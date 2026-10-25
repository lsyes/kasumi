#version 330 core

layout(location = 0) in vec4 a_rect;
layout(location = 1) in vec4 a_color;
layout(location = 2) in vec4 a_border_color;
layout(location = 3) in vec4 a_radii;
layout(location = 4) in vec4 a_uv;
layout(location = 5) in vec4 a_clip;
layout(location = 6) in vec4 a_meta;

uniform vec2 u_viewport;

out vec2 v_position;
out vec2 v_center;
out vec2 v_half_size;
flat out vec4 v_color;
flat out vec4 v_border_color;
flat out vec4 v_radii;
out vec2 v_uv;
flat out vec4 v_clip;
flat out vec4 v_meta;

const vec2 CORNERS[4] = vec2[4](
    vec2(0.0, 0.0),
    vec2(1.0, 0.0),
    vec2(0.0, 1.0),
    vec2(1.0, 1.0)
);

void main() {
    vec2 corner = CORNERS[gl_VertexID];
    vec2 position = a_rect.xy + corner * a_rect.zw;

    v_position = position;
    v_center = a_rect.xy + a_rect.zw * 0.5;
    v_half_size = a_rect.zw * 0.5;
    v_color = a_color;
    v_border_color = a_border_color;
    v_radii = a_radii;
    v_uv = a_uv.xy + corner * a_uv.zw;
    v_clip = a_clip;
    v_meta = a_meta;

    vec2 ndc = position / u_viewport * vec2(2.0, -2.0) + vec2(-1.0, 1.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
}
