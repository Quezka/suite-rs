# suite-rs

The Quezka suite rewritten in Rust (decided 2026-11-09): every app, desktop and phone, one by one, on egui + wgpu.
Order: the shared kit (`crates/kit`, quezka-kit) first, then Quire, then the rest. Data files, Quire's sync payloads and
the quire-server protocol must stay compatible with the Python and Kotlin apps they replace.

- Clean Architecture in every app (domain ← application ← infrastructure/presentation), enforced by a test, as in the
  Python apps. The user doesn't see well: big, high-contrast UI by default; high contrast never uses glass.
- Look: plain backgrounds, solid content, glass only on chrome over content. Never a gradient wash or colour blobs
  (the user called that "AI slop"). Corners nest: `theme::OUTER_RADIUS` 20 for outer surfaces, `GAP` 10 between them
  and from the window edge, `theme::nested(outer, padding)` inside.
- Glass (`crates/kit/src/glass.rs`, `glass.wgsl`): `glass::pane(painter, rect, radius, th, weight, floating)` adds a
  paint callback. `Compositor::render_scene` draws egui's paint list in pieces: what's before a pane into the scene
  texture, a dual-Kawase blur (4 levels), the pane (rounded SDF, tint, saturation, top rim), then the rest. Paint a
  pane's words after the pane. Chrome goes in a Foreground `Area` (with `.fade_in(false)`, or its words fade in while
  the glass doesn't) so it's painted after the content. The pane draws its own shadow (in the shader), so the glass
  never blurs its own shadow. `GlassPane.tint` is a Color32 (stored premultiplied): unmultiply before the shader.
- Kit modules: `theme` (tokens, radii), `fonts` (Manrope, `Weight`), `style` (egui visuals, 16 pt body), `icons`
  (Material Symbols via resvg; add names in `tools/make_symbols.py`), `widgets` (ported from Pillion's `m3.rs`, plus
  `sidebar_item`, `settings_card`, `setting_row`, `switch`), `glass`, `runner`. Examples: `glass_demo`, `settings_demo`
  (`--dark`, `--contrast`, `--scroll N`).
- Run: `cargo run --example glass_demo`. Check without a window: `--screenshot x.png [--dark] [--scroll 140]`
  (offscreen through the same compositor). egui is 0.37 (`run_ui`, panels take `&mut Ui`), wgpu 30
  (`InstanceDescriptor::new_*_display_handle`, `queue.present`).
- Commit as quezka <arsdom15@gmail.com>, never the Claude account's email. Remotes: `origin` github.com/Quezka/suite-rs
  (public, GPL-3.0-only), `lan` git.lan backup. Push both.
