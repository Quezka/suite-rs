// Liquid glass: a dual-Kawase blur of what has been drawn so far, and glass panes that show it through a rounded
// outline, lightly tinted, a little more colourful, with light catching their top edge.

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle that covers the whole target.
@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> VOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VOut;
    o.pos = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return o;
}

struct Blur {
    half_pixel: vec2<f32>,
    offset: f32,
    _pad: f32,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var<uniform> blur: Blur;

@fragment
fn fs_down(v: VOut) -> @location(0) vec4<f32> {
    let h = blur.half_pixel * blur.offset;
    var s = textureSample(src, samp, v.uv) * 4.0;
    s += textureSample(src, samp, v.uv - h);
    s += textureSample(src, samp, v.uv + h);
    s += textureSample(src, samp, v.uv + vec2<f32>(h.x, -h.y));
    s += textureSample(src, samp, v.uv - vec2<f32>(h.x, -h.y));
    return s / 8.0;
}

@fragment
fn fs_up(v: VOut) -> @location(0) vec4<f32> {
    let h = blur.half_pixel * blur.offset;
    var s = textureSample(src, samp, v.uv + vec2<f32>(-h.x * 2.0, 0.0));
    s += textureSample(src, samp, v.uv + vec2<f32>(-h.x, h.y)) * 2.0;
    s += textureSample(src, samp, v.uv + vec2<f32>(0.0, h.y * 2.0));
    s += textureSample(src, samp, v.uv + vec2<f32>(h.x, h.y)) * 2.0;
    s += textureSample(src, samp, v.uv + vec2<f32>(h.x * 2.0, 0.0));
    s += textureSample(src, samp, v.uv + vec2<f32>(h.x, -h.y)) * 2.0;
    s += textureSample(src, samp, v.uv + vec2<f32>(0.0, -h.y * 2.0));
    s += textureSample(src, samp, v.uv + vec2<f32>(-h.x, -h.y)) * 2.0;
    return s / 12.0;
}

@fragment
fn fs_copy(v: VOut) -> @location(0) vec4<f32> {
    return textureSample(src, samp, v.uv);
}

struct Pane {
    rect: vec4<f32>,    // min x, min y, max x, max y, in pixels
    screen: vec4<f32>,  // width, height, corner radius (pixels), 1 when dark
    tint: vec4<f32>,    // colour and how much of it
    look: vec4<f32>,    // saturation, rim strength, hairline strength, pixels per point
};

@group(0) @binding(0) var blurred: texture_2d<f32>;
@group(0) @binding(1) var bsamp: sampler;
@group(0) @binding(2) var<uniform> pane: Pane;

struct PaneOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) px: vec2<f32>,
};

@vertex
fn vs_pane(@builtin(vertex_index) i: u32) -> PaneOut {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let lo = pane.rect.xy - vec2<f32>(2.0);
    let hi = pane.rect.zw + vec2<f32>(2.0);
    let px = mix(lo, hi, corners[i]);
    var o: PaneOut;
    o.pos = vec4<f32>(px.x / pane.screen.x * 2.0 - 1.0, 1.0 - px.y / pane.screen.y * 2.0, 0.0, 1.0);
    o.px = px;
    return o;
}

@fragment
fn fs_pane(v: PaneOut) -> @location(0) vec4<f32> {
    let centre = (pane.rect.xy + pane.rect.zw) * 0.5;
    let half = (pane.rect.zw - pane.rect.xy) * 0.5;
    let r = min(pane.screen.z, min(half.x, half.y));
    let q = abs(v.px - centre) - half + vec2<f32>(r);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let cover = clamp(0.5 - d, 0.0, 1.0);
    if (cover <= 0.0) {
        discard;
    }
    var col = textureSample(blurred, bsamp, v.px / pane.screen.xy).rgb;
    let luma = dot(col, vec3<f32>(0.299, 0.587, 0.114));
    col = mix(vec3<f32>(luma), col, pane.look.x);
    col = mix(col, pane.tint.rgb, pane.tint.a);
    let inside = -d / pane.look.w;  // points from the edge
    let top = clamp((centre.y - v.px.y) / max(half.y, 1.0), 0.0, 1.0);
    let rim = (1.0 - smoothstep(0.0, 1.6, inside)) * top * pane.look.y;
    col = mix(col, vec3<f32>(1.0), rim);
    let edge = (1.0 - smoothstep(0.0, 1.0, inside)) * (1.0 - top) * pane.look.z;
    let ink = select(vec3<f32>(0.0), vec3<f32>(1.0), pane.screen.w > 0.5);
    col = mix(col, ink, edge);
    return vec4<f32>(col * cover, cover);
}
