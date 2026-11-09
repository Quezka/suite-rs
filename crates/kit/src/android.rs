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

/// Keeps the phone keyboard and the focused field in step. The keyboard starts from the field's real text and cursor;
/// what it changes becomes egui events (`typing::diff`); a cursor it moves (sliding on the space bar) moves the
/// field's cursor; a cursor moved on the field (by touch) is told to the keyboard.
#[derive(Default)]
pub struct Typing {
    field: Option<egui::Id>,
    last_text: String,
    last_cursor: (usize, usize),
}

fn span(cursor: (usize, usize)) -> TextSpan {
    TextSpan { start: cursor.0, end: cursor.1 }
}

impl Typing {
    /// Call once a frame, after it's drawn; returns the events to feed egui on the next frame.
    pub fn update(&mut self, app: &AndroidApp, ctx: &egui::Context) -> Vec<egui::Event> {
        let Some(field) = crate::typing::focused(ctx) else {
            if self.field.take().is_some() {
                app.hide_soft_input(true);
            }
            return Vec::new();
        };
        if self.field != Some(field.id) {
            self.field = Some(field.id);
            self.tell_keyboard(app, &field.text, field.cursor);
            app.show_soft_input(true);
            return Vec::new();
        }
        let keyboard = app.text_input_state();
        let keyboard_cursor = (keyboard.selection.start.min(keyboard.selection.end), keyboard.selection.start.max(keyboard.selection.end));
        if keyboard.text != self.last_text {
            let events = crate::typing::diff(&self.last_text, &keyboard.text);
            self.last_text = keyboard.text;
            self.last_cursor = keyboard_cursor;
            return events;
        }
        if keyboard_cursor != self.last_cursor {
            // The keyboard moved the cursor (the space-bar slide).
            self.last_cursor = keyboard_cursor;
            crate::typing::move_cursor(ctx, field.id, keyboard_cursor.0, keyboard_cursor.1);
            return Vec::new();
        }
        if field.text != self.last_text || field.cursor != self.last_cursor {
            // Changed on the field itself (a touch moved the cursor, a key on a hardware keyboard).
            self.tell_keyboard(app, &field.text, field.cursor);
        }
        Vec::new()
    }

    fn tell_keyboard(&mut self, app: &AndroidApp, text: &str, cursor: (usize, usize)) {
        self.last_text = text.to_string();
        self.last_cursor = cursor;
        app.set_text_input_state(TextInputState { text: text.to_string(), selection: span(cursor), compose_region: None });
    }
}
