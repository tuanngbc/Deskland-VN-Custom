#version 330 core
uniform vec4 uWhale;
uniform vec4 uAnim; // swim phase, surface amount, spray age, water clearance
uniform vec4 uDive; // body depth, tail depth, raised flukes, arch of the back
in vec2 vLocal;
out vec4 o;

float coverage(float d) { float aa=max(fwidth(d),0.6); return 1.0-smoothstep(-aa,aa,d); }
float ellipse(vec2 p,vec2 r) { return (length(p/r)-1.0)*min(r.x,r.y); }
float grain(vec2 p) { return fract(sin(dot(floor(p),vec2(127.1,311.7)))*43758.5453); }
void over(inout vec4 dst,vec3 color,float alpha) { dst=vec4(color*alpha,alpha)+dst*(1.0-alpha); }

// Rounded, tapering ribbon along a quadratic curve. Four short capsules give
// long flowing pectoral fins without triangles or a full-screen distance loop.
float finDistance(vec2 p,float flap) {
    vec2 a=vec2(13.0,12.0),b=vec2(3.0,31.0),c=vec2(-22.0,34.0+flap);
    float d=1000.0;
    for(int i=0;i<4;i++) {
        float t0=float(i)*0.25,t1=t0+0.25;
        vec2 start=mix(mix(a,b,t0),mix(b,c,t0),t0);
        vec2 end=mix(mix(a,b,t1),mix(b,c,t1),t1);
        vec2 v=end-start;
        float h=clamp(dot(p-start,v)/dot(v,v),0.0,1.0);
        float radius=mix(4.0,0.7,mix(t0,t1,h));
        d=min(d,length(p-start-v*h)-radius);
    }
    return d;
}

void main() {
    float c=cos(uWhale.z),s=sin(uWhale.z);
    vec2 raw=vec2(c*vLocal.x+s*vLocal.y,-s*vLocal.x+c*vLocal.y);
    float surface=uAnim.y,depth=uDive.x,tailDepth=uDive.y,lift=uDive.z,arch=uDive.w;
    float wet=smoothstep(65.0,145.0,uAnim.w);
    float swim=sin(uAnim.x),sway=sin(uAnim.x-0.9);
    float pitch=smoothstep(0.0,0.9,depth)*(1.0-tailDepth);
    vec2 q=raw;
    // The forebody shortens as the head tips down, while the back rises into
    // an arc and the tail keeps its own depth. This changes the silhouette.
    q.x=(q.x+54.0)/mix(1.0,0.74,pitch)-54.0;
    q.y/=mix(1.0,0.88,arch);
    q.y-=swim*pow(clamp((28.0-q.x)/100.0,0.0,1.0),2.0)*3.2;
    q.y-=arch*sin(clamp((q.x+60.0)/118.0,0.0,1.0)*3.14159)*3.5;
    float axial=smoothstep(-58.0,58.0,q.x);
    float localDepth=clamp(depth*1.25+axial*0.22-0.22,0.0,1.0);
    float bodyAlpha=(1.0-smoothstep(0.04,0.94,localDepth))*wet;
    float tailAlpha=(1.0-smoothstep(0.12,0.96,tailDepth))*wet;
    vec4 col=vec4(0.0);

    // Quiet, incomplete ripples instead of a hard white ring around the head.
    float bow=abs(ellipse(raw-vec2(27.0,0.0),vec2(37.0,23.0)))-0.65;
    float bowArc=smoothstep(36.0,59.0,raw.x)*(0.45+0.55*sin(raw.y*0.14+uAnim.x*0.6));
    float wake=abs(abs(raw.y)-(16.0+max(-raw.x-10.0,0.0)*0.20))-0.55;
    float wakeRange=smoothstep(-95.0,-60.0,raw.x)*(1.0-smoothstep(-18.0,5.0,raw.x));
    float broken=0.50+0.50*sin(raw.x*0.24+uAnim.x*0.9);
    over(col,vec3(0.69,0.91,0.89),(coverage(bow)*bowArc*0.20+coverage(wake)*wakeRange*broken*0.12)*surface*wet);

    // Broad, rounded head and a continuous taper into the tail peduncle.
    float width=mix(3.1,18.5,smoothstep(-58.0,5.0,q.x));
    float nose=clamp((q.x-20.0)/38.0,0.0,1.0);
    width*=sqrt(max(0.0,1.0-nose*nose));
    float body=max(abs(q.y)-width,max(-61.0-q.x,q.x-58.0));
    float fin=finDistance(vec2(q.x,abs(q.y)),sway*2.4-arch*3.0);
    float outline=min(body,fin);
    float n=grain(vLocal*uWhale.w*0.85)-0.5;
    // Muted blue-grey, fine grain and a soft spine highlight match the sea's
    // illustrated finish. Fins share the body colour rather than white facets.
    float roundness=sqrt(max(0.0,1.0-pow(clamp(q.y/max(width,1.0),-1.0,1.0),2.0)));
    vec3 skin=mix(vec3(0.18,0.34,0.37),vec3(0.35,0.50,0.51),roundness*0.64);
    skin+=vec3(0.024)*n;
    skin=mix(skin,vec3(0.25,0.65,0.68),smoothstep(0.1,0.85,localDepth)*0.82);
    float edge=coverage(outline+1.2)*(1.0-coverage(outline-0.2));
    over(col,vec3(0.42,0.77,0.77),edge*0.11*bodyAlpha);
    over(col,skin,coverage(outline)*bodyAlpha*0.90);
    // A gently curved dorsal ridge, with no dark triangular panel.
    float ridge=ellipse(q-vec2(-12.0,-1.0),vec2(9.0,2.2));
    over(col,vec3(0.17,0.31,0.34),coverage(ridge)*0.42*bodyAlpha);
    float spine=coverage(abs(q.y+2.4+sin(q.x*0.05)*0.8)-0.45)
        *smoothstep(-28.0,-16.0,q.x)*(1.0-smoothstep(23.0,40.0,q.x));
    over(col,vec3(0.56,0.66,0.63),spine*0.15*bodyAlpha);
    float blowhole=coverage(ellipse(q-vec2(34.0,0.0),vec2(2.0,0.85)));
    over(col,vec3(0.13,0.29,0.32),blowhole*0.65*bodyAlpha);

    // Crescent flukes with curved edges and rounded tips. During a sounding
    // dive they broaden, fold upward and remain above water after the back.
    vec2 tq=raw;
    tq.x=(tq.x+58.0-lift*10.0)/mix(1.0,0.64,lift);
    tq.y=(tq.y-swim*3.0)/mix(1.0,1.20,lift);
    float ft=clamp(abs(tq.y)/26.0,0.0,1.0);
    float center=-3.0-15.0*ft+7.0*sin(ft*3.14159);
    float chord=2.8*(1.0-ft)+6.5*sqrt(max(0.0,sin(ft*3.14159)));
    float flukes=max(abs(tq.x-center)-chord,abs(tq.y)-26.0);
    float stalk=ellipse(raw-vec2(-57.0+lift*8.0,swim*2.0),vec2(8.0,3.2));
    vec3 tailColor=mix(vec3(0.24,0.40,0.42),vec3(0.43,0.56,0.55),lift*0.60);
    tailColor=mix(tailColor,vec3(0.25,0.65,0.68),tailDepth*0.80);
    tailColor+=vec3(0.024)*n;
    over(col,tailColor,coverage(min(flukes,stalk))*tailAlpha*0.92);

    // Entry ripples outlive the back and spread while the flukes disappear.
    float entry=smoothstep(0.0,0.18,tailDepth)*(1.0-smoothstep(0.70,1.0,tailDepth));
    vec2 ring=raw-vec2(-47.0,0.0);
    float radius=16.0+tailDepth*27.0;
    float ripple=coverage(abs(ellipse(ring,vec2(radius,radius*0.46)))-0.65);
    ripple+=coverage(abs(ellipse(ring,vec2(radius+9.0,(radius+9.0)*0.46)))-0.45)*0.40;
    over(col,vec3(0.73,0.93,0.91),ripple*entry*0.25*wet);

    // Breath opens into two soft lobes and a few falling droplets, in this draw.
    float age=uAnim.z;
    if(age>0.0 && age<1.0 && wet>0.0) {
        vec2 anchor=vec2(c*34.0,s*34.0);
        vec2 p=vLocal-anchor;
        float grow=smoothstep(0.0,0.22,age),fade=1.0-smoothstep(0.48,1.0,age);
        float height=52.0*grow,up=-p.y;
        float u=clamp(up/max(height,1.0),0.0,1.0);
        float drift=age*7.0*u;
        float width=1.4+u*u*10.0;
        float envelope=smoothstep(-1.0,4.0,up)*(1.0-smoothstep(height-7.0,height+6.0,up));
        float mist=(1.0-smoothstep(width*0.15,width,abs(p.x-drift)))*envelope;
        float split=(1.0-smoothstep(1.2,5.5,abs(abs(p.x-drift)-u*u*7.0)))*envelope;
        over(col,vec3(0.89,0.96,0.93),(mist*0.26+split*0.23)*fade*grow*wet);
        for(int i=0;i<7;i++) {
            float f=float(i),a=f*2.39996;
            float life=clamp((age-0.14)*1.4,0.0,1.0);
            vec2 drop=vec2(cos(a)*(5.0+f*1.6)*life+age*5.0,
                           -height*(0.65+0.04*f)+sin(a)*5.0+life*life*18.0);
            float d=ellipse(p-drop,vec2(0.65+f*0.04,1.2));
            over(col,vec3(0.88,0.97,0.95),coverage(d)*0.36*fade*grow*wet);
        }
    }
    o=col;
}
