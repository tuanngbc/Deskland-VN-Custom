#version 330 core
// Audited diamond/ribbon artwork mathematics; native world coordinates/lifetime.
uniform vec2 origin,world,pixelScale,hand,kite;
uniform float scale,time,reaction;
out vec4 o;
float segment(vec2 p,vec2 a,vec2 b){vec2 v=b-a;return length(p-a-v*clamp(dot(p-a,v)/max(dot(v,v),.001),0.,1.));}
void main(){
    vec2 p=vec2(gl_FragCoord.x/pixelScale.x,world.y-gl_FragCoord.y/pixelScale.y)+origin;
    vec2 k=kite+vec2(sin(time*.7)*18.,cos(time*.9)*12.-reaction*35.)*scale;
    vec2 attach=k+vec2(-28.,-8.)*scale,previous=hand;
    float line=1e5;
    for(int i=1;i<=16;i++){float t=float(i)/16.;vec2 point=mix(hand,attach,t)+vec2(0.,sin(t*3.14159)*22.*scale);line=min(line,segment(p,previous,point));previous=point;}
    vec4 result=vec4(.46,.40,.25,(1.-smoothstep(.25,.85,line))*.8);
    float angle=-.23+sin(time*.7)*.09+reaction*.2,c=cos(angle),s=sin(angle);
    vec2 delta=(p-k)/max(scale,.01),q=vec2(c*delta.x+s*delta.y,-s*delta.x+c*delta.y);
    if(q.y>58.&&q.y<245.)for(int j=0;j<3;j++){
        float y=q.y-65.,x=2.+float(j)*8.+sin(y*.055-time*3.+float(j)*.5)*(8.+y*.06);
        float ribbon=(1.-smoothstep(1.,2.,abs(q.x-x)))*(1.-smoothstep(160.,180.,y));
        result=mix(result,vec4(j==0?vec3(.88,.65,.37):j==1?vec3(.81,.39,.30):vec3(.32,.55,.62),1.),ribbon);
    }
    float diamond=abs(q.x)/53.+abs(q.y)/70.;float coverage=1.-smoothstep(.97,1.025,diamond);
    vec3 fabric=q.y<0.?(q.x<0.?vec3(.95,.77,.42):vec3(.88,.43,.33)):(q.x<0.?vec3(.94,.77,.44):vec3(.35,.57,.64));
    fabric=mix(fabric,vec3(.46,.40,.25),smoothstep(.93,1.,diamond)*.78);
    fabric=mix(fabric,vec3(.98,.92,.71),(1.-smoothstep(.35,.9,min(abs(q.x),abs(q.y))))*.65);
    o=mix(result,vec4(fabric,1.),coverage);if(o.a<.001)discard;
}
