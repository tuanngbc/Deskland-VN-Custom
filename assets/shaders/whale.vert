#version 330 core
uniform vec2 uRes;
uniform vec4 uWhale; // scene x/y, heading, size
out vec2 vLocal;
void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vLocal = mix(vec2(-110.0,-150.0), vec2(110.0,100.0), corner);
    vec2 p = uWhale.xy + vLocal*uWhale.w;
    gl_Position = vec4(p.x/uRes.x*2.0-1.0, 1.0-p.y/uRes.y*2.0, 0.0, 1.0);
}
