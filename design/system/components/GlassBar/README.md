# GlassBar

A floating bar over scrolling content: its words stay solid, the content under it shows through blurred.

Only over content that moves under it. Tint `glass-tint`, `shadow-float`, light rim on top. Solid in high contrast.

The consumer supplies the words (translated) and what happens on press. In code: `quezka_kit::widgets` (Rust), `quezka_ui.widgets` / `settings` (Python), `ui/` (Kotlin).
