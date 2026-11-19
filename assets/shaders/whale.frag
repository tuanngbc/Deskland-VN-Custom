#version 330 core
in vec3 vNormal;
in vec3 vSkin;
in float vHeight;
in float vPart;
out vec4 o;
float hash(vec2 p) { return fract(sin(dot(floor(p),vec2(127.1,311.7)))*43758.5453); }
void main() {
    float depth=max(-vHeight,0.0);
    float opacity=exp(-depth*0.063)*(1.0-smoothstep(54.0,77.0,depth));
    if(opacity<0.008)discard;
    vec3 n=normalize(vNormal);
    float light=clamp(dot(n,normalize(vec3(-0.35,-0.50,0.86)))*0.5+0.5,0.0,1.0);
    float belly=1.0-smoothstep(-0.48,0.05,vSkin.z/max(length(vSkin.yz),1.0));
    vec3 charcoal=mix(vec3(0.12,0.23,0.25),vec3(0.29,0.40,0.41),light);
    float mottling=hash(vSkin.xy*0.55)*0.70+hash(vSkin.xy*0.13)*0.30;
    vec3 color=mix(charcoal,vec3(0.66,0.73,0.70),belly*0.80);
    if(vPart>0.5&&vPart<2.5) {
        float pale=smoothstep(0.64,0.88,mottling)*belly;
        color=mix(color,vec3(0.75,0.81,0.76),pale*0.72);
    }
    color+=(mottling-0.5)*0.025;
    float wet=1.0-exp(-depth*0.045);
    color=mix(color,vec3(0.08,0.60,0.66),wet*0.82);
    float waterline=exp(-abs(vHeight)*1.8)*0.11;
    color=mix(color,vec3(0.73,0.91,0.87),waterline);
    o=vec4(color*opacity,opacity);
}
