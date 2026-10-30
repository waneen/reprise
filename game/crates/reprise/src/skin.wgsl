// LR2 スキンの四角形（頂点色 × 画像）。合成モードごとに出力を変え、ブレンドはパイプライン側で設定する
#import bevy_sprite::{
    mesh2d_vertex_output::VertexOutput,
    mesh2d_view_bindings::view,
}
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping
#endif
#ifdef SRGB_OUTPUT
#import bevy_render::color_operations::linear_to_srgb
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var texture_sampler: sampler;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let t = textureSample(texture, texture_sampler, mesh.uv);
    var c = vec4(1.0);
#ifdef VERTEX_COLORS
    c = mesh.color;
#endif
#ifdef BLEND_INV_SRC
    var out = vec4((vec3(1.0) - t.rgb) * c.rgb, t.a * c.a);
#else
    var out = t * c;
#endif
#ifdef BLEND_NONE
    out.a = t.a;
#endif
#ifdef TONEMAP_IN_SHADER
    out = tonemapping::tone_mapping(out, view.color_grading);
#endif
#ifdef SRGB_OUTPUT
    out = vec4(linear_to_srgb(out.rgb), out.a);
#endif
#ifdef BLEND_MUL
    out = vec4(mix(vec3(1.0), out.rgb, out.a), out.a);
#endif
#ifdef BLEND_PREMUL
    out = vec4(out.rgb * out.a, out.a);
#endif
    return out;
}
