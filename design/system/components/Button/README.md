# Button

One button per kind of action: filled for the screen's main action, tonal for secondary ones, outlined or text for quiet ones, danger only for deleting.

Labels are verbs (Save, Go here), `label` type, at least 44px tall. One filled button per screen at most. Ask for confirmation only when nothing can undo the action.

The consumer supplies the words (translated) and what happens on press. In code: `quezka_kit::widgets` (Rust), `quezka_ui.widgets` / `settings` (Python), `ui/` (Kotlin).
