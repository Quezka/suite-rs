# suite-rs

The Quezka suite rewritten in Rust (decided 2026-11-09): every app, desktop and phone, one by one, on egui + wgpu.
Order: the shared kit (`crates/kit`, quezka-kit) first, then Quire, then the rest. Data files, Quire's sync payloads and
the quire-server protocol must stay compatible with the Python and Kotlin apps they replace.

- Clean Architecture in every app (domain ← application ← infrastructure/presentation), enforced by a test, as in the
  Python apps. The user doesn't see well: big, high-contrast UI by default; high contrast never uses glass.
- Look (2026-11-14, user): Material 3 tonal, NO glass (as Python kit 0.13); headers solid `th.raised` once scrolled; `glass.rs` legacy, no new use. Plain backgrounds, solid content. Never a gradient wash or colour blobs
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
- Android: GameActivity (winit `android-game-activity`), because NativeActivity can't carry the keyboard's text.
  The shell is a small Gradle project (`crates/kit-demo-android/android/gradle-shell`: games-activity 4.4.0,
  edge-to-edge theme, the Rust `.so` + `libc++_shared.so` in jniLibs). `kit::android` reads the system bars,
  cutout and keyboard insets over JNI (jni 0.22, `jni_str!`/`jni_sig!`) → `quezka_kit::safe_area(ctx)`; typing
  mirrors GameActivity's text state and `typing::diff` turns changes into egui Backspace/Text events.
  `crates/kit-demo-android` (`android_main` → `quezka_kit::run_android`); build with
  `bash crates/kit-demo-android/android/build.sh` (arm64, throwaway key in ~/.local/share/quezka-kitdemo). It
  scrolls by itself and logs frame times: `adb logcat -s quezka`. On the user's Redmi Note 11 Pro+ (Mali-G68,
  Vulkan, 60 Hz) it holds ~61 fps with two glass panes (2026-11-09). MIUI blocks `adb install` unless "Install via
  USB" is on: push the APK to /sdcard/Download and let the user install it. On the real phone never send gestures
  into the user's own apps (a swipe once deleted notes).
- Quire (`crates/quire`): Clean Architecture (`domain`, `application`, `infrastructure`; `tests/architecture.rs`).
  Done: every domain rule (model, payroll, water, medicine, sleep, focus, register, ical; money removed 2026-11-14); the SQLite
  schema v20 + migrations, identical to the Python app's (either app can open the same file); the sync store
  (triggers, outgoing/apply/mark_sent/pending/mark_all_dirty/clear_all), byte-compatible records. Parity:
  `python3 tools/quire_fixtures.py` (needs ../quire and its venv) writes `crates/quire/tests/fixtures/*`, and
  `payroll_parity`, `rules_parity`, `storage_parity` check the Rust answers against the Python ones. Python's
  `round()` is `round_ties_even` (ints) or `model::round2` (2 places, via formatting); payroll's round2 adds 1e-9.
  Next: repositories, application services, cloud backends (Firebase, quire-server), then the screens.

- Quay (`crates/quay`, 2026-11-09): the suite launcher, rewritten from Python. Same layers (tests/architecture.rs),
  `curl` for HTTP (no TLS stack), `dpkg-query`/`pkexec apt-get` on Linux, `reg` + the Inno setup on Windows, real app
  icons bundled from each repo (`tools/quay_icons.sh`, drawn with `quezka_kit::icons::picture`). `quay --demo`,
  `--screenshot x.png [--dark]`, `--write-icon x.png|x.ico` (packaging), `QUAY_LANG=it|ru`. Released from the
  Quezka/quay repo, whose CI builds this crate at the commit in its `suite-rs.ref`.

- App icons: `tools/make_app_icons.py` -> `icons/apps/` (svg + android fg/bg); one template, plain colour, white Material Symbol; copy into each repo + Quay bundle.
- Quay 0.7 (2026-11-15): filter chips, 2 card columns >= 1180 wide, buttons under text in cards < 560, one header button when narrow.
- Kit runner `Options.close_hides` (2026-11-15): window X hides, app runs on unseen (250 ms tick); `ViewportCommand::Close` quits; `Visible(true)` reopens a window on the same gpu (textures kept).
- Tether 0.2 (2026-11-15): desktop layout (sidebar | section; messages = list | bubbles; < 760 wide -> tabs), single instance via `$XDG_RUNTIME_DIR/tether.sock` (2nd launch -> show), autostart on first run (`tether --background`), drop files -> phone, Ctrl+1..4, `--demo --section N --thread ADDR`.
- Quire port progress (2026-11-16): application layer started: `application/{errors,bus,ports,tasks,ideas,planner,digest,notes,timetable,water}.rs` (planner day agenda = Python, `tests/agenda_parity.rs` + fixture `agenda.json`), repositories `infrastructure/repositories.rs` (Sqlite{Settings,Courses,Tasks,Ideas}, same SQL/order as Python), `domain/ideas.rs`; tests `tests/services.rs`. Services hold `Rc<dyn Port>` (single-threaded, UI thread). Next: planner (agenda), notes, focus/sleep/water/medicine services, digest, sync service + cloud.
