//! Typing through a phone keyboard: the keyboard edits its own copy of the text (with autocorrect and composing), and
//! each change has to reach egui as the keys egui understands. The kit's text fields (`text_field`, `text_area`) say
//! what the focused field holds and where its cursor is, so the keyboard starts from the real text: then sliding on
//! the space bar moves the cursor through all of it, and moving the cursor by touch tells the keyboard.

/// The field being typed in: its id, its text and its cursor (character positions, start and end of the selection).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusedText {
    pub id: egui::Id,
    pub text: String,
    pub cursor: (usize, usize),
}

fn focus_key() -> egui::Id {
    egui::Id::NULL.with("quezka-focused-text")
}

/// What the focused kit field held at the end of the last frame, if a kit field has focus.
pub fn focused(ctx: &egui::Context) -> Option<FocusedText> {
    ctx.data(|d| d.get_temp::<FocusedText>(focus_key()))
}

/// Puts the cursor (or a selection) of field `id` at these character positions.
pub fn move_cursor(ctx: &egui::Context, id: egui::Id, start: usize, end: usize) {
    if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, id) {
        let range = egui::text_selection::CCursorRange::two(egui::text::CCursor::new(start), egui::text::CCursor::new(end));
        state.cursor.set_char_range(Some(range));
        state.store(ctx, id);
        ctx.request_repaint();
    }
}

fn record(ui: &egui::Ui, output: &egui::text_edit::TextEditOutput, text: &str) {
    let ctx = ui.ctx();
    if output.response.response.has_focus() {
        let cursor = output.cursor_range.map_or((0, 0), |r| {
            let (a, b) = (r.primary.index.0, r.secondary.index.0);
            (a.min(b), a.max(b))
        });
        ctx.data_mut(|d| d.insert_temp(focus_key(), FocusedText { id: output.response.response.id, text: text.to_string(), cursor }));
    } else if focused(ctx).is_some_and(|f| f.id == output.response.response.id) {
        ctx.data_mut(|d| d.remove::<FocusedText>(focus_key()));
    }
}

/// A one-line text box that works with phone keyboards.
pub fn text_field(ui: &mut egui::Ui, text: &mut String, hint: &str) -> egui::Response {
    let output = egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY).show(ui);
    record(ui, &output, text);
    output.response.response
}

/// A text area of `rows` lines that works with phone keyboards.
pub fn text_area(ui: &mut egui::Ui, text: &mut String, hint: &str, rows: usize, font: egui::FontId) -> egui::Response {
    let output = egui::TextEdit::multiline(text).hint_text(hint).desired_width(f32::INFINITY).desired_rows(rows).font(font).show(ui);
    record(ui, &output, text);
    output.response.response
}

/// The events that turn `before` into `after`: backspaces back to where they part, then the new text.
pub fn diff(before: &str, after: &str) -> Vec<egui::Event> {
    let shared = before.chars().zip(after.chars()).take_while(|(a, b)| a == b).count();
    let removed = before.chars().count() - shared;
    let mut events = Vec::new();
    for _ in 0..removed {
        for pressed in [true, false] {
            events.push(egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    let added: String = after.chars().skip(shared).collect();
    if !added.is_empty() {
        events.push(egui::Event::Text(added));
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay(start: &str, events: &[egui::Event]) -> String {
        let mut text: Vec<char> = start.chars().collect();
        for e in events {
            match e {
                egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => {
                    text.pop();
                }
                egui::Event::Text(t) => text.extend(t.chars()),
                _ => {}
            }
        }
        text.into_iter().collect()
    }

    #[test]
    fn typing_autocorrect_and_deleting_end_up_as_the_keyboard_has_it() {
        for (before, after) in [("", "hello"), ("helo", "hello "), ("hello", "help"), ("Привет", "Привет, мир"), ("abc", ""), ("same", "same")] {
            assert_eq!(replay(before, &diff(before, after)), after, "{before} -> {after}");
        }
        assert!(diff("same", "same").is_empty());
    }
}
