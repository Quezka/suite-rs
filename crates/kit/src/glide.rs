//! Trackpad flick keeps scrolling after fingers lift (gnome/wayland send no momentum).
//! Same rules as python kit `kinetic.py`: speed of last `SAMPLE_MS` at lift, decays by `GLIDE_MS`.
//! Only point-unit wheel events with a phase = trackpad; mouse wheel (line unit) never glides.

use egui::{Event, MouseWheelUnit, TouchPhase, Vec2};

const GLIDE_MS: f64 = 325.0;
const SAMPLE_MS: f64 = 100.0;
const STILL_MS: f64 = 80.0;
const START_SPEED: f32 = 0.3; // points per ms
const STOP_SPEED: f32 = 0.02;
const MAX_SPEED: f32 = 8.0;

#[derive(Default)]
pub struct Glide {
    samples: Vec<(f64, Vec2)>,
    speed: Vec2,
    last: f64,
    modifiers: egui::Modifiers,
}

impl Glide {
    pub fn active(&self) -> bool {
        self.speed != Vec2::ZERO
    }

    /// Look at this frame's input (`now` in ms); a lift starts a glide, a touch/click/key stops it.
    pub fn feed(&mut self, now: f64, events: &[Event]) {
        for event in events {
            match event {
                Event::MouseWheel { unit: MouseWheelUnit::Point, delta, phase, modifiers, source } => {
                    if *source == egui::MouseWheelSource::Momentum || modifiers.ctrl || modifiers.command {
                        self.stop();
                        continue;
                    }
                    match phase {
                        TouchPhase::Start => {
                            self.stop();
                            self.samples.clear();
                        }
                        TouchPhase::Move => {
                            self.stop();
                            if *delta != Vec2::ZERO {
                                self.samples.push((now, *delta));
                                self.modifiers = *modifiers;
                            }
                        }
                        TouchPhase::End | TouchPhase::Cancel => self.lift(now),
                    }
                }
                Event::MouseWheel { .. } | Event::PointerButton { pressed: true, .. } | Event::Key { pressed: true, .. } => {
                    self.stop();
                    self.samples.clear();
                }
                _ => {}
            }
        }
    }

    fn lift(&mut self, now: f64) {
        let recent: Vec<_> = self.samples.drain(..).filter(|(t, _)| now - t <= SAMPLE_MS).collect();
        if recent.len() < 2 || now - recent.last().unwrap().0 > STILL_MS {
            return;
        }
        let span = (now - recent[0].0).max(16.0) as f32;
        let sum = recent.iter().fold(Vec2::ZERO, |a, (_, d)| a + *d);
        let mut speed = sum / span;
        // one axis only <- diagonal drift feels wrong
        if speed.x.abs() > speed.y.abs() { speed.y = 0.0 } else { speed.x = 0.0 }
        if speed.length() < START_SPEED {
            return;
        }
        self.speed = speed.clamp(Vec2::splat(-MAX_SPEED), Vec2::splat(MAX_SPEED));
        self.last = now;
    }

    pub fn stop(&mut self) {
        self.speed = Vec2::ZERO;
    }

    /// The scroll to add this frame, as a wheel event; None when not gliding.
    pub fn tick(&mut self, now: f64) -> Option<Event> {
        if !self.active() {
            return None;
        }
        let elapsed = (now - self.last).max(1.0);
        self.last = now;
        self.speed *= (-elapsed / GLIDE_MS).exp() as f32;
        if self.speed.length() < STOP_SPEED {
            self.stop();
            return None;
        }
        Some(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: self.speed * elapsed as f32,
            phase: TouchPhase::Move,
            source: egui::MouseWheelSource::Momentum,
            modifiers: self.modifiers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel(y: f32, phase: TouchPhase) -> Event {
        Event::MouseWheel { unit: MouseWheelUnit::Point, delta: Vec2::new(0.0, y), phase, source: egui::MouseWheelSource::Unknown, modifiers: Default::default() }
    }

    fn flick(g: &mut Glide) -> f64 {
        let mut t = 0.0;
        g.feed(t, &[wheel(0.0, TouchPhase::Start)]);
        for _ in 0..6 {
            t += 16.0;
            g.feed(t, &[wheel(-20.0, TouchPhase::Move)]);
        }
        t += 16.0;
        g.feed(t, &[wheel(0.0, TouchPhase::End)]);
        t
    }

    #[test]
    fn flick_glides_then_dies_down() {
        let mut g = Glide::default();
        let mut t = flick(&mut g);
        assert!(g.active());
        let mut total = 0.0;
        for _ in 0..400 {
            t += 16.0;
            match g.tick(t) {
                Some(Event::MouseWheel { delta, .. }) => total += delta.y,
                _ => break,
            }
        }
        assert!(total < -100.0 && !g.active());
    }

    #[test]
    fn mouse_wheel_and_clicks_never_glide() {
        let mut g = Glide::default();
        let t = flick(&mut g);
        g.feed(t, &[Event::MouseWheel { unit: MouseWheelUnit::Line, delta: Vec2::new(0.0, -1.0), phase: TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: Default::default() }]);
        assert!(!g.active());
    }

    #[test]
    fn resting_before_lift_means_stop() {
        let mut g = Glide::default();
        g.feed(0.0, &[wheel(-20.0, TouchPhase::Move)]);
        g.feed(16.0, &[wheel(-20.0, TouchPhase::Move)]);
        g.feed(300.0, &[wheel(0.0, TouchPhase::End)]);
        assert!(!g.active());
    }
}
