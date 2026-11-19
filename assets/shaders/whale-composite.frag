#version 330 core
uniform sampler2D uImage;
uniform vec2 uUvSize;
in vec2 vUv;
out vec4 o;
void main() {o=texture(uImage,clamp(vUv,vec2(0.5/512.0),uUvSize-vec2(0.5/512.0)));}
