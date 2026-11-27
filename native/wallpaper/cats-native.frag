// Native scene composition using audited cat-hill GLSL artwork functions.
// The CPU tiny-skia renderer/runtime is not used. New controls affect composition.
uniform float wpMound, wpFlowers, wpBushes, wpSun, wpActorScale;
uniform vec4 wpTrees[12]; // stable ascending ground depth, computed once on CPU
uniform float wpTreeN;
void main() {
  vec2 p=vec2(gl_FragCoord.x,uRes.y-gl_FragCoord.y);
  float W=uRes.x,H=uRes.y,s=wpActorScale;
  vec3 col=mix(vec3(.70,.86,.98),vec3(.91,.97,1.),p.y/H);
  col=mix(col,vec3(1.,.94,.63),ss(31.*s,29.*s,length(p-vec2(W*.79,H*.13)))*clamp(wpSun,0.,1.));
  for(int i=0;i<4;i++)drawCloud(col,p,vec2(mod(float(i)*W*.31+uTime*4.,W+160.)-80.,H*(.10+float(i%2)*.08)),.7*s);
  float hill=H*(.30-.025*sin(p.x/W*6.28)*wpMound/28.);
  vec3 grass=mix(vec3(.72,.88,.49),vec3(.46,.72,.31),clamp((p.y-hill)/(H-hill),0.,1.));
  col=mix(col,grass,ss(1.,-1.,hill-p.y));
  for(int i=0;i<12;i++) {
    float fi=float(i),x=W*(.06+hash2(vec2(fi,8.))*.88),y=H*(.4+hash2(vec2(fi,10.))*.47);
    if(fi<wpFlowers*8.){vec2 q=p-vec2(x,y);float d=length(q);col=mix(col,vec3(1.,.8,.91),ss(5.*s,3.*s,d));col=mix(col,vec3(1.,.9,.3),ss(2.*s,1.*s,d));}
  }
  int tree=0,cat=0;bool toyDrawn=uToy.w<=0.;
  // Merge ground objects by foot depth. Airborne leaves remain the final pass.
  for(int layer=0;layer<28;layer++){
    float ty=float(tree)<wpTreeN?wpTrees[tree].y:1.e20;
    float cy=float(cat)<uCatN?uCatPos[cat].y:1.e20;
    float yy=toyDrawn?1.e20:uToy.y;
    if(min(min(ty,cy),yy)>1.e19)break;
    if(ty<=cy&&ty<=yy){vec4 tr=wpTrees[tree++];drawMidTree(col,p,tr.xy,tr.z*s,sin(uTime+tr.w)*3.*uWind);}
    else if(cy<=yy){drawCat(col,p,uCatPos[cat],uCatMeta[cat]);cat++;}
    else {drawYarnBall(col,p,uToy,s);toyDrawn=true;}
  }
  for(int i=0;i<6;i++){if(float(i)>=uBflyN)break;vec4 b=uBfly[i];vec2 q=(p-b.xy)/s;float flap=.2+.8*abs(sin(uTime*8.+float(i)));float d=min(sdEllipse(q-vec2(3.,0.),vec2(4.*flap,5.)),sdEllipse(q+vec2(3.,0.),vec2(4.*flap,5.)));strokeFill(col,d*s,vec3(1.,.9,.35),OUTLINE,1.,1.);}
  for(int i=0;i<28;i++){if(float(i)>=uLeafN)break;vec4 leaf=uLeaf[i];vec2 q=rot(leaf.z)*(p-leaf.xy)/s;strokeFill(col,sdEllipse(q,vec2(5.,2.5))*s,vec3(.55,.78,.3),OUTLINE,.8,1.);}
  o=vec4(col,1.);
}
