#version 330 core
in vec3 vNormal;
in vec3 vSkin;
in float vHeight;
in float vPart;
in float vBelly;
uniform vec2 uDetail;
out vec4 o;
float hash(vec2 p) { return fract(sin(dot(floor(p),vec2(127.1,311.7)))*43758.5453); }
void main() {
    float depth=max(-vHeight,0.0);
    float opacity=exp(-depth*0.052)*(1.0-smoothstep(60.0,84.0,depth))*uDetail.x;
    if(opacity<0.008)discard;
    vec3 n=normalize(vNormal);
    float light=clamp(dot(n,normalize(vec3(-0.35,-0.50,0.86)))*0.5+0.5,0.0,1.0);
    float belly=vBelly;
    vec3 charcoal=mix(vec3(0.035,0.042,0.049),vec3(0.16,0.18,0.20),light);
    float mottling=hash(vSkin.xy*0.55)*0.70+hash(vSkin.xy*0.13)*0.30;
    vec3 ivory=vec3(0.78,0.79,0.77)*(0.64+0.36*light);
    vec3 color=mix(charcoal,ivory,belly*0.88);
    if(vPart>0.5&&vPart<1.5) {
        // Long pale pectorals with dark roots/tips, common in Atlantic humpbacks.
        float patch=smoothstep(15.5,23.0,abs(vSkin.y))*(1.0-smoothstep(51.0,56.0,abs(vSkin.y)));
        color=mix(color,ivory,patch*(0.82+0.10*mottling));
    } else if(vPart>1.5&&vPart<2.5) {
        color=mix(charcoal,ivory,belly*smoothstep(0.28,0.65,mottling)*0.90);
    }
    color+=(mottling-0.5)*0.009;
    float wet=1.0-exp(-depth*0.045);
    color=mix(color,vec3(0.035,0.43,0.49),wet*0.70);
    float waterline=exp(-abs(vHeight)*1.8)*0.07;
    color=mix(color,vec3(0.73,0.91,0.87),waterline);
    o=vec4(color*opacity,opacity);
}
