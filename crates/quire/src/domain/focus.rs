//! Pomodoro focus timer: the rules, independent of any clock or UI (port of `quire/domain/focus.py`). Time is always
//! passed in, so the timer is exact (it measures against the wall clock rather than counting ticks) and testable.

use chrono::{Duration, NaiveDateTime};

use super::errors::{Checked, ValidationError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Work,
    ShortBreak,
    LongBreak,
}

impl Phase {
    pub fn code(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::ShortBreak => "short_break",
            Self::LongBreak => "long_break",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusSettings {
    pub work_minutes: i64,
    pub short_break_minutes: i64,
    pub long_break_minutes: i64,
    /// Focus sessions before a long break.
    pub rounds: u32,
    /// Start the next phase by itself.
    pub auto_continue: bool,
}

impl Default for FocusSettings {
    fn default() -> Self {
        Self { work_minutes: 25, short_break_minutes: 5, long_break_minutes: 15, rounds: 4, auto_continue: true }
    }
}

impl FocusSettings {
    pub fn validate(&self) -> Checked {
        if !(1..=180).contains(&self.work_minutes) {
            return Err(ValidationError("Focus must last between 1 and 180 minutes."));
        }
        if !(1..=60).contains(&self.short_break_minutes) {
            return Err(ValidationError("Short break must last between 1 and 60 minutes."));
        }
        if !(1..=120).contains(&self.long_break_minutes) {
            return Err(ValidationError("Long break must last between 1 and 120 minutes."));
        }
        if !(1..=12).contains(&self.rounds) {
            return Err(ValidationError("Use between 1 and 12 rounds before a long break."));
        }
        Ok(())
    }

    pub fn minutes(&self, phase: Phase) -> i64 {
        match phase {
            Phase::Work => self.work_minutes,
            Phase::ShortBreak => self.short_break_minutes,
            Phase::LongBreak => self.long_break_minutes,
        }
    }
}

/// Reported by `tick` when a phase runs out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhaseEnded {
    pub phase: Phase,
    pub started: NaiveDateTime,
    pub minutes: i64,
    pub next_phase: Phase,
}

/// A focus session, for the stats. One cut short (skipped or reset) isn't `complete`: its minutes count towards your
/// focus time, but it isn't counted as a pomodoro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusSession {
    pub started: NaiveDateTime,
    pub minutes: i64,
    pub task_id: Option<i64>,
    /// What it was spent on: the task's title or an ad-hoc project.
    pub label: String,
    pub complete: bool,
}

fn whole_minutes(d: Duration) -> i64 {
    (d.num_milliseconds() as f64 / 60_000.0).round_ties_even() as i64
}

#[derive(Clone, Debug)]
pub struct PomodoroTimer {
    pub settings: FocusSettings,
    pub phase: Phase,
    /// Focus sessions finished in the current cycle.
    pub completed: u32,
    length: Duration,
    remaining: Duration,
    ends_at: Option<NaiveDateTime>,
    started_at: Option<NaiveDateTime>,
}

impl PomodoroTimer {
    pub fn new(settings: FocusSettings) -> Self {
        let length = Duration::minutes(settings.work_minutes);
        Self { settings, phase: Phase::Work, completed: 0, length, remaining: length, ends_at: None, started_at: None }
    }

    pub fn running(&self) -> bool {
        self.ends_at.is_some()
    }

    /// Whether the current phase has begun (it may be paused now).
    pub fn started(&self) -> bool {
        self.started_at.is_some()
    }

    /// How long the current phase lasts (set when it began, even if settings change).
    pub fn duration(&self) -> Duration {
        self.length
    }

    /// Time spent in the current phase so far, not counting pauses.
    pub fn elapsed(&self, now: NaiveDateTime) -> Duration {
        if self.started() { self.length - self.remaining(now) } else { Duration::zero() }
    }

    /// The unfinished focus session that ending the phase now would leave, if any. Only focus phases count, and only
    /// a whole minute or more of them.
    pub fn cut_short(&self, now: NaiveDateTime) -> Option<FocusSession> {
        if self.phase != Phase::Work {
            return None;
        }
        let started = self.started_at?;
        let minutes = whole_minutes(self.elapsed(now));
        (minutes >= 1).then(|| FocusSession { started, minutes, task_id: None, label: String::new(), complete: false })
    }

    pub fn remaining(&self, now: NaiveDateTime) -> Duration {
        match self.ends_at {
            None => self.remaining,
            Some(end) => (end - now).max(Duration::zero()),
        }
    }

    /// 0.0 at the start of the phase, 1.0 when it's over.
    pub fn progress(&self, now: NaiveDateTime) -> f64 {
        let total = self.length.num_milliseconds() as f64;
        if total == 0.0 { 1.0 } else { 1.0 - self.remaining(now).num_milliseconds() as f64 / total }
    }

    /// Which focus session of the cycle this is (1-based).
    pub fn round(&self) -> u32 {
        (self.completed + u32::from(self.phase == Phase::Work)).min(self.settings.rounds)
    }

    pub fn start(&mut self, now: NaiveDateTime) {
        if self.running() {
            return;
        }
        self.started_at.get_or_insert(now);
        self.ends_at = Some(now + self.remaining);
    }

    pub fn pause(&mut self, now: NaiveDateTime) {
        if self.running() {
            self.remaining = self.remaining(now);
            self.ends_at = None;
        }
    }

    /// Back to a fresh focus session; the cycle starts over.
    pub fn reset(&mut self) {
        self.phase = Phase::Work;
        self.completed = 0;
        self.enter(Phase::Work, None);
    }

    /// Jump to the next phase without counting the current one as done.
    pub fn skip(&mut self, now: NaiveDateTime) {
        let from = self.running().then_some(now);
        let next = self.next_phase(false, false);
        self.enter(next, from);
    }

    /// New durations take effect from the next phase, or now if nothing started yet.
    pub fn apply_settings(&mut self, settings: FocusSettings) -> Checked {
        settings.validate()?;
        self.settings = settings;
        if self.started_at.is_none() {
            self.length = Duration::minutes(self.settings.minutes(self.phase));
            self.remaining = self.length;
        }
        Ok(())
    }

    /// Advance if the running phase has run out; report what ended.
    pub fn tick(&mut self, now: NaiveDateTime) -> Option<PhaseEnded> {
        let ended_at = self.ends_at?;
        if now < ended_at {
            return None;
        }
        let event = PhaseEnded {
            phase: self.phase,
            started: self.started_at.unwrap_or(ended_at - self.length),
            minutes: whole_minutes(self.length),
            next_phase: self.next_phase(true, true),
        };
        let next = self.next_phase(true, false);
        let from = self.settings.auto_continue.then_some(ended_at);
        self.enter(next, from);
        Some(event)
    }

    fn next_phase(&mut self, counting: bool, dry_run: bool) -> Phase {
        if self.phase != Phase::Work {
            if !dry_run && self.phase == Phase::LongBreak {
                self.completed = 0;
            }
            return Phase::Work;
        }
        let done = self.completed + u32::from(counting);
        if !dry_run {
            self.completed = done;
        }
        if done >= self.settings.rounds { Phase::LongBreak } else { Phase::ShortBreak }
    }

    fn enter(&mut self, phase: Phase, running_from: Option<NaiveDateTime>) {
        self.phase = phase;
        self.length = Duration::minutes(self.settings.minutes(phase));
        self.remaining = self.length;
        self.started_at = running_from;
        self.ends_at = running_from.map(|t| t + self.remaining);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(minute: i64) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 10).unwrap().and_hms_opt(9, 0, 0).unwrap() + Duration::minutes(minute)
    }

    #[test]
    fn a_full_cycle_ends_in_a_long_break() {
        let mut timer = PomodoroTimer::new(FocusSettings { rounds: 2, ..FocusSettings::default() });
        timer.start(at(0));
        let first = timer.tick(at(25)).unwrap();
        assert_eq!((first.phase, first.next_phase, first.minutes), (Phase::Work, Phase::ShortBreak, 25));
        assert_eq!(timer.tick(at(30)).unwrap().next_phase, Phase::Work);
        assert_eq!(timer.tick(at(55)).unwrap().next_phase, Phase::LongBreak);
        assert_eq!(timer.round(), 2);
        timer.tick(at(70));
        assert_eq!((timer.phase, timer.completed), (Phase::Work, 0));
    }

    #[test]
    fn pausing_stops_the_clock_and_cut_short_counts_minutes() {
        let mut timer = PomodoroTimer::new(FocusSettings::default());
        timer.start(at(0));
        timer.pause(at(10));
        assert_eq!(timer.remaining(at(60)), Duration::minutes(15));
        let cut = timer.cut_short(at(60)).unwrap();
        assert_eq!((cut.minutes, cut.complete), (10, false));
    }
}
