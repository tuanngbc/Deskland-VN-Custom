#version 330 core
uniform sampler2D scene;
uniform vec2 size,origin,world;
uniform float time,hour,rain,snow,cloud,wind,lighting;
out vec4 o;
float hash(vec2 p){p=fract(p*vec2(123.34,456.21));p+=dot(p,p+45.32);return fract(p.x*p.y);}
float noise(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);return mix(mix(hash(i),hash(i+vec2(1,0)),f.x),mix(hash(i+vec2(0,1)),hash(i+vec2(1)),f.x),f.y);}
void main(){
  vec2 uv=gl_FragCoord.xy/size,p=vec2(uv.x*size.x,(1.-uv.y)*size.y)+origin;
  vec3 color=texture(scene,uv).rgb;
  if(lighting>.5){float daylight=smoothstep(5.,8.,hour)*(1.-smoothstep(17.,21.,hour));color*=mix(vec3(.83,.88,1.),vec3(1.),daylight);}
  if(cloud>0.){float c=noise(p/450.+vec2(time*.008*wind,0));color*=1.-c*cloud*.16;}
  if(rain>0.){vec2 q=p+vec2(time*170.*wind,time*650.);vec2 cell=floor(q/vec2(25.,80.));vec2 f=fract(q/vec2(25.,80.));float drop=step(.68,hash(cell))*smoothstep(.05,.01,abs(f.x-.5))*smoothstep(.35,.08,abs(f.y-.5));color=mix(color,vec3(.78,.86,.95),drop*rain*.65);}
  if(snow>0.){for(int i=0;i<2;i++){float layer=float(i)+1.;vec2 q=p/vec2(28.,35.)/layer+vec2(time*.1*wind,-time*.3)/layer;vec2 cell=floor(q),f=fract(q);float flake=smoothstep(.13,.035,length(f-vec2(hash(cell),hash(cell+1.))))*step(.5,hash(cell+2.));color=mix(color,vec3(.99),flake*snow*.8);}}
  o=vec4(color,1.);
}
