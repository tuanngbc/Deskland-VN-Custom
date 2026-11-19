#version 330 core
in vec3 vNormal;
in vec3 vSkin;
in float vHeight;
in float vPart;
in float vBelly;
in float vFacing;
uniform vec2 uDetail;
uniform float uEdgeAA;
out vec4 o;
float hash(vec2 p) { return fract(sin(dot(floor(p),vec2(127.1,311.7)))*43758.5453); }
void main() {
    float depth=max(-vHeight,0.0);
    float opacity=exp(-depth*0.052)*(1.0-smoothstep(60.0,84.0,depth))*uDetail.x;
    if(uEdgeAA>0.0)opacity*=smoothstep(0.0,max(fwidth(vFacing)*1.4,0.025),vFacing);
    if(opacity<0.008)discard;
    vec3 n=normalize(vNormal);
    float light=clamp(dot(n,normalize(vec3(-0.35,-0.50,0.86)))*0.5+0.5,0.0,1.0);
    float belly=vBelly;
    vec3 charcoal=mix(vec3(0.055,0.060,0.066),vec3(0.22,0.235,0.25),light*light);
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
    if(vPart<0.5) {
        // A broad rostrum, paired blowholes, small lateral eyes and a curved
        // jaw crease make the head read as a baleen whale at desktop scale.
        float top=smoothstep(0.2,0.8,n.z);
        float eye=length((vec2(vSkin.x-43.0,abs(vSkin.y)-13.9))/vec2(1.15,0.8));
        float holes=length((vec2(vSkin.x-38.0,abs(vSkin.y)-1.15))/vec2(1.8,0.65));
        float edge=max(fwidth(eye),0.15);
        color=mix(color,vec3(0.025), (1.0-smoothstep(0.7,1.0+edge,eye))*0.85);
        color=mix(color,vec3(0.035), (1.0-smoothstep(0.6,1.1,holes))*top*0.8);
        float ridge=exp(-vSkin.y*vSkin.y/10.0)*smoothstep(32.0,41.0,vSkin.x)*(1.0-smoothstep(61.0,67.0,vSkin.x));
        color+=vec3(0.028)*ridge*top;
        float seam=abs(vSkin.z+0.8+(vSkin.x-50.0)*0.035)-0.22;
        color*=1.0-(1.0-smoothstep(-fwidth(seam),fwidth(seam)+0.15,seam))*smoothstep(32.0,39.0,vSkin.x)*0.25;
    }
    float wet=1.0-exp(-depth*0.045);
    color=mix(color,vec3(0.035,0.43,0.49),wet*0.70);
    float waterline=exp(-abs(vHeight)*1.8)*0.07;
    color=mix(color,vec3(0.73,0.91,0.87),waterline);
    o=vec4(color*opacity,opacity);
}
