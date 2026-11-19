#version 330 core
uniform vec4 uWhale;
uniform vec4 uAnim; // swim phase, surface amount, spray age, water clearance
in vec2 vLocal;
out vec4 o;

float coverage(float d) { float aa=max(fwidth(d),0.65); return 1.0-smoothstep(-aa,aa,d); }
float ellipse(vec2 p, vec2 r) { return (length(p/r)-1.0)*min(r.x,r.y); }
float segment(vec2 p,vec2 a,vec2 b) {
    vec2 v=b-a; return length(p-a-v*clamp(dot(p-a,v)/dot(v,v),0.0,1.0));
}
float tri(vec2 p,vec2 a,vec2 b,vec2 c) {
    vec2 ab=b-a,bc=c-b,ca=a-c;
    vec2 pa=p-a,pb=p-b,pc=p-c;
    float orient=sign(ab.x*(c.y-a.y)-ab.y*(c.x-a.x));
    float inside=min(min(orient*(ab.x*pa.y-ab.y*pa.x),orient*(bc.x*pb.y-bc.y*pb.x)),orient*(ca.x*pc.y-ca.y*pc.x));
    return min(segment(p,a,b),min(segment(p,b,c),segment(p,c,a)))*(inside>=0.0?-1.0:1.0);
}
void over(inout vec4 dst,vec3 color,float alpha) { dst=vec4(color*alpha,alpha)+dst*(1.0-alpha); }

void main() {
    float c=cos(uWhale.z),s=sin(uWhale.z);
    vec2 q=vec2(c*vLocal.x+s*vLocal.y,-s*vLocal.x+c*vLocal.y);
    float wave=sin(uAnim.x);
    q.y-=wave*pow(clamp((32.0-q.x)/100.0,0.0,1.0),2.0)*5.0;
    float surface=uAnim.y;
    float wet=smoothstep(65.0,145.0,uAnim.w);
    float opacity=mix(0.19,0.94,surface)*wet;
    vec4 col=vec4(0.0);

    // Slow, broken bow waves and a widening wake, confined to this small quad.
    float bow=abs(ellipse(q-vec2(34.0,0.0),vec2(31.0,23.0)))-0.75;
    float wake=abs(abs(q.y)-(14.0+max(-q.x-12.0,0.0)*0.30))-0.55;
    float wakeAlpha=coverage(wake)*smoothstep(-90.0,-58.0,q.x)*(1.0-smoothstep(-12.0,5.0,q.x));
    float bowAlpha=coverage(bow)*smoothstep(28.0,46.0,q.x);
    over(col,vec3(0.78,0.97,0.97),(wakeAlpha*0.25+bowAlpha*0.42)*surface*wet);

    // Humpback silhouette seen from above: broad rostrum, tapered peduncle,
    // long pectoral fins, and two swept flukes with a real central notch.
    float body=ellipse(q-vec2(10.0,0.0),vec2(43.0,16.5));
    body=min(body,tri(q,vec2(-22.0,-13.0),vec2(-56.0,-3.5),vec2(-56.0,3.5)));
    body=min(body,tri(q,vec2(-22.0,-13.0),vec2(-56.0,3.5),vec2(-22.0,13.0)));
    vec2 tail=q-vec2(-55.0,wave*2.0);
    float flukes=min(tri(tail,vec2(3.0,0.0),vec2(-13.0,-25.0),vec2(-18.0,-8.0)),
                     tri(tail,vec2(3.0,0.0),vec2(-13.0,25.0),vec2(-18.0,8.0)));
    flukes=min(flukes,tri(tail,vec2(3.0,0.0),vec2(-18.0,-8.0),vec2(-9.0,0.0)));
    flukes=min(flukes,tri(tail,vec2(3.0,0.0),vec2(-9.0,0.0),vec2(-18.0,8.0)));
    vec2 fin=vec2(q.x,abs(q.y));
    float fins=tri(fin,vec2(16.0,10.0),vec2(-15.0,43.0+wave*2.0),vec2(-4.0,15.0));
    float outline=min(body,min(flukes,fins));
    float halo=coverage(outline-2.0)*(1.0-coverage(outline));
    over(col,vec3(0.36,0.80,0.83),halo*0.30*opacity);
    float light=clamp(0.56-q.y*0.023,0.0,1.0);
    vec3 skin=mix(vec3(0.075,0.19,0.25),vec3(0.27,0.43,0.48),light);
    skin=mix(skin,vec3(0.45,0.71,0.73),0.36*(1.0-surface));
    float finTip=smoothstep(22.0,40.0,abs(q.y))*coverage(fins);
    skin=mix(skin,vec3(0.72,0.84,0.83),finTip*0.83);
    over(col,skin,coverage(outline)*opacity);
    // A thin highlight along the spine and the darker raised dorsal fin.
    float ridge=coverage(abs(q.y+3.0)-0.55)*smoothstep(-25.0,-12.0,q.x)*(1.0-smoothstep(21.0,38.0,q.x));
    over(col,vec3(0.51,0.66,0.66),ridge*0.32*opacity);
    float dorsal=tri(q,vec2(-11.0,0.0),vec2(-27.0,-7.0),vec2(-22.0,2.0));
    over(col,vec3(0.08,0.18,0.22),coverage(dorsal)*opacity);
    float blowhole=coverage(ellipse(q-vec2(36.0,0.0),vec2(2.5,1.1)));
    over(col,vec3(0.04,0.13,0.18),blowhole*opacity);

    // Upright breath: a narrow stem opens into two soft fans and falling drops.
    // No live particle allocations; all droplets use this same bounded draw.
    float age=uAnim.z;
    if(age>0.0 && age<1.0 && wet>0.0) {
        vec2 anchor=vec2(c*36.0,s*36.0);
        vec2 p=vLocal-anchor;
        float grow=smoothstep(0.0,0.26,age);
        float fade=1.0-smoothstep(0.52,1.0,age);
        float height=60.0*grow;
        float up=-p.y;
        float u=clamp(up/max(height,1.0),0.0,1.0);
        float width=1.4+pow(u,2.3)*15.0;
        float envelope=smoothstep(-2.0,4.0,up)*(1.0-smoothstep(height-10.0,height+5.0,up));
        float center=sin(up*0.065+uAnim.x*0.8)*u*2.0+age*6.0*u;
        float mist=(1.0-smoothstep(width*0.20,width,abs(p.x-center)))*envelope;
        float split=(1.0-smoothstep(2.0,8.0,abs(abs(p.x-center)-u*u*10.0)))*envelope;
        over(col,vec3(0.89,0.98,0.99),(mist*0.28+split*0.30)*fade*grow*wet);
        for(int i=0;i<10;i++) {
            float f=float(i),a=f*2.39996;
            float life=clamp((age-0.12)*1.4,0.0,1.0);
            vec2 drop=vec2(cos(a)*(7.0+f*1.5)*life+age*5.0,
                           -height*(0.60+0.035*f)+sin(a)*6.0+life*life*22.0);
            float d=ellipse(p-drop,vec2(0.7+f*0.045,1.5));
            over(col,vec3(0.91,0.99,1.0),coverage(d)*0.50*fade*grow*wet);
        }
    }
    o=col;
}
