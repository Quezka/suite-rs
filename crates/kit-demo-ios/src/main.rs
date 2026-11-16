//! iOS entry: winit runs UIApplicationMain itself, so a plain `main` works.
//! build: `crates/kit-demo-ios/build_ipa.sh` (macOS + Xcode) -> unsigned `.ipa` -> AltStore | Sideloadly.

fn main() {
    if let Err(e) = quezka_kit::run("Kit demo", kitdemo::Demo::default()) {
        eprintln!("kit demo: {e}");
    }
}
