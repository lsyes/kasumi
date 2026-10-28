struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) uv: vec4<f32>,
    @location(5) clip: vec4<f32>,
    @location(6) kind_data: vec4<f32>,
}

struct Uniforms {
    viewport: vec2<f32>,
    _padding: vec2<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) rect: vec4<f32>,
    @location(1) local_position: vec2<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) color: vec4<f32>,
    @location(4) @interpolate(flat) border_color: vec4<f32>,
    @location(5) @interpolate(flat) radii: vec4<f32>,
    @location(6) @interpolate(flat) clip: vec4<f32>,
    @location(7) @interpolate(flat) kind_data: vec4<f32>,
}

@vertex
fn vs_main(instance: Instance, @builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 4>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = corners[vertex_index];
    let position = instance.rect.xy + corner * instance.rect.zw;
    var output: VertexOutput;
    let ndc = vec2<f32>(
        position.x / uniforms.viewport.x * 2.0 - 1.0,
        1.0 - position.y / uniforms.viewport.y * 2.0,
    );
    output.position = vec4<f32>(ndc, 0.0, 1.0);
    output.rect = instance.rect;
    output.local_position = corner * instance.rect.zw;
    output.uv = instance.uv.xy + corner * instance.uv.zw;
    output.color = instance.color;
    output.border_color = instance.border_color;
    output.radii = instance.radii;
    output.clip = instance.clip;
    output.kind_data = instance.kind_data;
    return output;
}

fn sd_rounded_box(point: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    let radius = select(
        select(radii.x, radii.y, point.y < 0.0),
        select(radii.w, radii.z, point.y < 0.0),
        point.x < 0.0,
    );
    let clamped_radius = clamp(radius, 0.0, min(half_size.x, half_size.y));
    let q = abs(point) - half_size + vec2<f32>(clamped_radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - clamped_radius;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let absolute = input.rect.xy + input.local_position;
    if (
        absolute.x < input.clip.x || absolute.y < input.clip.y ||
        absolute.x >= input.clip.x + input.clip.z ||
        absolute.y >= input.clip.y + input.clip.w
    ) {
        discard;
    }

    let position = input.local_position;
    let size = input.rect.zw;

    let kind = u32(input.kind_data.x + 0.5);
    let half_size = size * 0.5;
    let point = position - half_size;
    if kind == 0u {
        let distance = sd_rounded_box(point, half_size, input.radii);
        let aa = max(fwidth(distance), 0.0001);
        let coverage = clamp(0.5 - distance / aa, 0.0, 1.0);
        let border_width = input.kind_data.y;
        var border_alpha = 0.0;
        var border_rgb = vec3<f32>(0.0);
        var fill_coverage = coverage;
        if border_width > 0.0 && input.border_color.a > 0.0 {
            let inner_half = max(half_size - vec2<f32>(border_width), vec2<f32>(0.0));
            let inner_radii = max(input.radii - vec4<f32>(border_width), vec4<f32>(0.0));
            let inner_distance = sd_rounded_box(point, inner_half, inner_radii);
            let inner_coverage = clamp(0.5 - inner_distance / aa, 0.0, 1.0);
            border_alpha = max(coverage - inner_coverage, 0.0) * input.border_color.a;
            border_rgb = input.border_color.rgb * border_alpha;
            fill_coverage = inner_coverage;
        }
        let background_alpha = fill_coverage * input.color.a;
        let background_rgb = input.color.rgb * background_alpha;
        let rgb = border_rgb + background_rgb * (1.0 - border_alpha);
        let alpha = border_alpha + background_alpha * (1.0 - border_alpha);
        return vec4<f32>(rgb, alpha);
    } else if kind == 1u {
        let mask = textureSample(atlas_texture, atlas_sampler, input.uv).r;
        let alpha = mask * input.color.a;
        return vec4<f32>(input.color.rgb * alpha, alpha);
    } else if kind == 2u {
        let sprite = textureSample(atlas_texture, atlas_sampler, input.uv);
        let alpha = sprite.a * input.color.a;
        return vec4<f32>(sprite.rgb * alpha, alpha);
    } else {
        let sprite = textureSample(atlas_texture, atlas_sampler, input.uv);
        let distance = sd_rounded_box(point, half_size, input.radii);
        let aa = max(fwidth(distance), 0.0001);
        let coverage = clamp(0.5 - distance / aa, 0.0, 1.0);
        let alpha = sprite.a * input.color.a * coverage;
        return vec4<f32>(sprite.rgb * sprite.a * input.color.a * coverage, alpha);
    }
}
