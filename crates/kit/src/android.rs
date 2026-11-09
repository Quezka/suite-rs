//! What a phone needs beyond a window: where the system bars and the keyboard cover the screen (so words stay clear
//! of them while glass runs under them), and typing through the keyboard (GameActivity's text input, turned into
//! egui's text and backspace events).

use egui::Margin;
use jni::objects::{JObject, JValue};
use jni::{jni_sig, jni_str, JavaVM};
use winit::platform::android::activity::input::{TextInputState, TextSpan};
use winit::platform::android::activity::AndroidApp;

/// Pixels covered at each edge: left, top, right, bottom (the bottom includes the keyboard when it's up).
pub fn insets(app: &AndroidApp) -> Option<[i32; 4]> {
    // SAFETY: android-activity hands out the process's JavaVM and the activity's global reference; both outlive this.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    vm.attach_current_thread(|env| -> jni::errors::Result<[i32; 4]> {
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr().cast()) };
        let window = env.call_method(&activity, jni_str!("getWindow"), jni_sig!("()Landroid/view/Window;"), &[])?.l()?;
        let decor = env.call_method(&window, jni_str!("getDecorView"), jni_sig!("()Landroid/view/View;"), &[])?.l()?;
        let insets = env
            .call_method(&decor, jni_str!("getRootWindowInsets"), jni_sig!("()Landroid/view/WindowInsets;"), &[])?
            .l()?;
        if insets.is_null() {
            return Ok([0; 4]);
        }
        let kind = |env: &mut jni::Env, name: &'static jni::strings::JNIStr| -> jni::errors::Result<i32> {
            env.call_static_method(jni_str!("android/view/WindowInsets$Type"), name, jni_sig!("()I"), &[])?.i()
        };
        let mask = kind(env, jni_str!("systemBars"))? | kind(env, jni_str!("displayCutout"))? | kind(env, jni_str!("ime"))?;
        let edges = env
            .call_method(&insets, jni_str!("getInsets"), jni_sig!("(I)Landroid/graphics/Insets;"), &[JValue::Int(mask)])?
            .l()?;
        let mut out = [0; 4];
        for (slot, name) in out.iter_mut().zip([jni_str!("left"), jni_str!("top"), jni_str!("right"), jni_str!("bottom")]) {
            *slot = env.get_field(&edges, name, jni_sig!("I"))?.i()?;
        }
        Ok(out)
    })
    .ok()
}

/// The safe area as egui margins, in points.
pub fn margins(pixels: [i32; 4], pixels_per_point: f32) -> Margin {
    let p = |v: i32| (v as f32 / pixels_per_point).round().clamp(0.0, 127.0) as i8;
    Margin { left: p(pixels[0]), top: p(pixels[1]), right: p(pixels[2]), bottom: p(pixels[3]) }
}

/// Mirrors the keyboard's text for the field being typed in. The keyboard edits its own copy (with autocorrect and
/// composing); each change becomes backspaces for what it took away and text for what it added, as egui expects.
#[derive(Default)]
pub struct Typing {
    active: bool,
    last: String,
}

impl Typing {
    /// Call once a frame with whether egui wants typing; returns the events to feed egui on the next frame.
    pub fn update(&mut self, app: &AndroidApp, wants_keyboard: bool) -> Vec<egui::Event> {
        if wants_keyboard && !self.active {
            self.active = true;
            self.last.clear();
            app.set_text_input_state(TextInputState { text: String::new(), selection: TextSpan { start: 0, end: 0 }, compose_region: None });
            app.show_soft_input(true);
            return Vec::new();
        }
        if !wants_keyboard {
            if self.active {
                self.active = false;
                app.hide_soft_input(true);
            }
            return Vec::new();
        }
        let now = app.text_input_state().text;
        let events = crate::typing::diff(&self.last, &now);
        self.last = now;
        events
    }
}
