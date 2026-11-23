import bevy_ui_render::ui_vertex_output::UiVertexOutput;

@group(1) @binding(0) var surface_texture: texture_2d<f32>;
@group(1) @binding(1) var surface_sampler: sampler;
// x: node pixels per texel; y: node pixels per logical pixel. Zero stretches.
@group(1) @binding(2) var<uniform> scale: vec2<f32>;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    var uv = in.uv;
    var inside = true;
    if scale.x > 0.0 {
        // The size the surface was painted for. CEF truncates the view to
        // whole logical pixels, so within one it is the node's own.
        let painted = vec2<f32>(textureDimensions(surface_texture)) * scale.x;
        let other = abs(painted - in.size) > vec2<f32>(scale.y + 0.5);
        uv = select(uv, uv * in.size / painted, other);
        inside = all(uv <= vec2<f32>(1.0));
    }
    // The surface is Bgra8UnormSrgb; the format handles channel order + sRGB,
    // so a plain sample yields correct RGBA (mirrors the mesh material).
    let color = textureSampleLevel(surface_texture, surface_sampler, uv, 0.0);
    return select(vec4<f32>(0.0), color, inside);
}
