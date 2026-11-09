//! Typing through a phone keyboard: the keyboard edits its own copy of the text (with autocorrect and composing), and
//! each change has to reach egui as the keys egui understands.

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
