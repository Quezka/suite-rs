# Quezka suite in Rust

The Quezka apps, rewritten in Rust on egui + wgpu, starting with the shared kit: the suite's colours and shapes, and
liquid glass with a real backdrop blur (what's under a glass bar shows through it, blurred, as on Apple's and Windows'
own glass).

    cargo run --example glass_demo
    cargo run --example glass_demo -- --screenshot glass.png --dark --scroll 140

GPL-3.0-only.
