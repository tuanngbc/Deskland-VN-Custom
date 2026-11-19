#version 330 core
uniform vec2 uRes;
uniform vec4 uRect;
uniform vec2 uUvSize;
out vec2 vUv;
void main() {
    vec2 corner=vec2(float(gl_VertexID&1),float(gl_VertexID>>1));
    vec2 p=uRect.xy+corner*uRect.zw;
    gl_Position=vec4(p.x/uRes.x*2.0-1.0,1.0-p.y/uRes.y*2.0,0.0,1.0);
    vUv=vec2(corner.x,1.0-corner.y)*uUvSize;
}
