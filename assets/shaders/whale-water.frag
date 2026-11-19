#version 330 core
uniform vec4 uWhale;
uniform vec4 uPose;
uniform vec4 uEffect; // breath age, fluke-entry age, surface amount
in vec2 vLocal;
out vec4 o;
float coverage(float d) { return 1.0-smoothstep(-max(fwidth(d),0.7),max(fwidth(d),0.7),d); }
float ellipse(vec2 p,vec2 r) {return (length(p/r)-1.0)*min(r.x,r.y);}
void over(inout vec4 dst,vec3 color,float a) {dst=vec4(color*a,a)+dst*(1.0-a);}
void main() {
    float c=cos(uWhale.z),s=sin(uWhale.z);
    vec2 q=vec2(c*vLocal.x+s*vLocal.y,-s*vLocal.x+c*vLocal.y);
    vec4 color=vec4(0.0);
    float surface=uEffect.z;
    float wake=abs(abs(q.y)-(18.0+max(-q.x,0.0)*0.20))-0.55;
    float along=smoothstep(-110.0,-67.0,q.x)*(1.0-smoothstep(-12.0,13.0,q.x));
    over(color,vec3(0.69,0.91,0.88),coverage(wake)*along*surface*0.13);
    float entry=uEffect.y;
    if(entry>0.0&&entry<1.0) {
        float r=17.0+entry*38.0;
        float ring=coverage(abs(ellipse(q-vec2(-40.0,0.0),vec2(r,r*0.52)))-0.60);
        ring+=coverage(abs(ellipse(q-vec2(-40.0,0.0),vec2(r+10.0,(r+10.0)*0.52)))-0.45)*0.35;
        over(color,vec3(0.73,0.92,0.88),ring*sin(entry*3.14159)*0.19);
    }
    float age=uEffect.x;
    if(age>0.0&&age<1.0) {
        float x=42.0*cos(uPose.y)-9.0*sin(uPose.y);
        float z=42.0*sin(uPose.y)+9.0*cos(uPose.y)+uPose.x;
        vec2 p=vLocal-vec2(c*x,s*x-z*0.62);
        float grow=smoothstep(0.0,0.23,age),fade=1.0-smoothstep(0.42,1.0,age);
        float height=57.0*grow,up=-p.y;
        float t=clamp(up/max(height,1.0),0.0,1.0);
        float drift=age*8.0*t;
        float width=1.3+t*t*12.0;
        float envelope=smoothstep(-1.0,4.0,up)*(1.0-smoothstep(height-8.0,height+7.0,up));
        float mist=(1.0-smoothstep(width*0.15,width,abs(p.x-drift)))*envelope;
        float split=(1.0-smoothstep(1.2,5.0,abs(abs(p.x-drift)-t*t*7.0)))*envelope;
        over(color,vec3(0.89,0.96,0.92),(mist*0.27+split*0.22)*fade*grow);
        for(int i=0;i<7;i++) {
            float f=float(i),a=f*2.39996,life=clamp((age-0.15)*1.4,0.0,1.0);
            vec2 drop=vec2(cos(a)*(5.0+f*1.5)*life+age*5.0,-height*(0.64+0.04*f)+sin(a)*5.0+life*life*18.0);
            over(color,vec3(0.87,0.96,0.93),coverage(ellipse(p-drop,vec2(0.65,1.2)))*0.33*fade*grow);
        }
    }
    o=color*uEffect.w;
}
