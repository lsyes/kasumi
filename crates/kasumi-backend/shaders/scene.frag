#version 330 core

in vec2 v_position;
in vec2 v_center;
in vec2 v_half_size;
flat in vec4 v_color;
flat in vec4 v_border_color;
flat in vec4 v_radii;
in vec2 v_uv;
flat in vec4 v_clip;
flat in vec4 v_meta;

uniform sampler2D u_atlas;

out vec4 out_color;

float sd_rounded_box(vec2 point, vec2 half_size, vec4 radii) {
    float radius = point.x < 0.0
        ? (point.y < 0.0 ? radii.x : radii.w)
        : (point.y < 0.0 ? radii.y : radii.z);
    radius = clamp(radius, 0.0, min(half_size.x, half_size.y));
    vec2 q = abs(point) - half_size + vec2(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - radius;
}

void main() {
    if (v_position.x < v_clip.x || v_position.y < v_clip.y ||
        v_position.x >= v_clip.x + v_clip.z ||
        v_position.y >= v_clip.y + v_clip.w) {
        discard;
    }

    int kind = int(v_meta.x + 0.5);

    if (kind == 0) {
        vec2 point = v_position - v_center;
        float distance = sd_rounded_box(point, v_half_size, v_radii);
        float aa = max(fwidth(distance), 0.0001);
        float coverage = clamp(0.5 - distance / aa, 0.0, 1.0);

        float border_width = v_meta.y;
        float border_alpha = 0.0;
        vec3 border_rgb = vec3(0.0);

        if (border_width > 0.0 && v_border_color.a > 0.0) {
            vec2 inner_half = max(v_half_size - vec2(border_width), vec2(0.0));
            vec4 inner_radii = max(v_radii - vec4(border_width), vec4(0.0));
            float inner_distance = sd_rounded_box(point, inner_half, inner_radii);
            float inner_coverage = clamp(0.5 - inner_distance / aa, 0.0, 1.0);
            float band = max(coverage - inner_coverage, 0.0);
            border_alpha = band * v_border_color.a;
            border_rgb = v_border_color.rgb * border_alpha;
            coverage = inner_coverage;
        }

        float background_alpha = coverage * v_color.a;
        vec3 background_rgb = v_color.rgb * background_alpha;

        // Composite the border over the fill, in premultiplied space.
        vec3 rgb = border_rgb + background_rgb * (1.0 - border_alpha);
        float alpha = border_alpha + background_alpha * (1.0 - border_alpha);
        out_color = vec4(rgb, alpha);
    } else if (kind == 1) {
        float mask = texture(u_atlas, v_uv).r;
        float alpha = mask * v_color.a;
        out_color = vec4(v_color.rgb * alpha, alpha);
    } else if (kind == 2) {
        vec4 sprite = texture(u_atlas, v_uv);
        float alpha = sprite.a * v_color.a;
        out_color = vec4(sprite.rgb * alpha, alpha);
    } else {
        vec4 sprite = texture(u_atlas, v_uv);
        vec2 point = v_position - v_center;
        float distance = sd_rounded_box(point, v_half_size, v_radii);
        float aa = max(fwidth(distance), 0.0001);
        float coverage = clamp(0.5 - distance / aa, 0.0, 1.0);
        float alpha = sprite.a * v_color.a * coverage;
        out_color = vec4(sprite.rgb * sprite.a * v_color.a * coverage, alpha);
    }
}
