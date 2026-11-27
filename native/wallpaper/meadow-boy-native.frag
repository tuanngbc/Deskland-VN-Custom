#version 300 es
precision highp float;
uniform sampler2D uCharacter, uRunSprite;
uniform vec2 uRes, uOffset, uKite, uBody, uLegs, uHand, uReelHand, uTug;
uniform vec4 uPose, uMotion;
uniform vec4 uLeftHipKnee, uRightHipKnee, uRig;
uniform vec2 uLeftAnkle, uRightAnkle, uFootAngles;
uniform vec2 uGround;
uniform vec4 uContactL, uContactR;
uniform vec4 uPlace;
uniform float uScale, uTime, uLook, uShadow, uRunWind, uTension, uKiteRock, wpShadowAlpha;
out vec4 o;
vec4 over(vec4 base, vec4 top) {
  float alpha=top.a+base.a*(1.-top.a);
  return vec4((top.rgb*top.a+base.rgb*base.a*(1.-top.a))/max(alpha,.001),alpha);
}
float segment(vec2 p, vec2 a, vec2 b) {
  vec2 v=b-a;return length(p-a-v*clamp(dot(p-a,v)/max(dot(v,v),.001),0.,1.));
}
vec2 turn(vec2 p,float angle) {
  float c=cos(angle),s=sin(angle);return vec2(c*p.x-s*p.y,s*p.x+c*p.y);
}
vec2 bodySpace(vec2 p) {
  return p-uBody*clamp((78.-p.y)/170.,0.,1.);
}
float armMask(vec2 p) {
  float boundary=p.y < -30. ? -38.+(-p.y-30.)*.10 : min(-17.,-38.+(p.y+30.)*.78);
  return (1.-smoothstep(boundary-.45,boundary+.45,p.x))*(1.-smoothstep(0.,3.,p.y));
}
vec4 spriteColor(vec4 color,vec2 p) {
  color.a*=step(-76.,p.x)*step(-95.,p.y)*step(p.x,76.)*step(p.y,85.);
  return color;
}
vec2 spriteUV(vec2 p) {
  return clamp((p+vec2(76.,95.))/vec2(152.,180.),.002,.998);
}
vec2 personSource(vec2 p) {
  vec2 boy=bodySpace(p);
  vec2 rest=boy;
  float lower=smoothstep(24.,48.,rest.y);
  float right=smoothstep(-4.,9.,rest.x);
  float lift=mix(uLegs.x,uLegs.y,right);
  boy.y+=lift*lower;
  boy.x-=(right*2.-1.)*lift*.24*lower;
  return boy;
}
float ribbon(vec2 p,float offset,float phase) {
  float t=clamp(p.y/108.,0.,1.);
  float gust=sin(t*5.6-uTime*4.8+phase)*uRunWind*9.+sin(t*6.-uTime*7.+phase)*abs(uKiteRock)*12.;
  float center=offset+50.*t+(12.*sin(t*4.8-uTime*1.15+phase)+gust)*t;
  float width=mix(3.4,1.6,t);
  return (1.-smoothstep(width,width+1.,abs(p.x-center)))*step(0.,p.y)*step(p.y,108.);
}
float kiteAngle() {
  return -.23+sin(uTime*.7)*.09+sin(uTime*4.8+.7)*uRunWind*.24+uKiteRock;
}
vec4 kiteColor(vec2 p,vec2 screen) {
  vec4 result=vec4(0.);
  float angle=kiteAngle();
  vec2 delta=p-uKite;
  vec2 q=vec2(cos(angle)*delta.x+sin(angle)*delta.y,-sin(angle)*delta.x+cos(angle)*delta.y);
  vec2 tail=q-vec2(2.,65.);
  result=over(result,vec4(.88,.65,.37,ribbon(tail,0.,0.)));
  result=over(result,vec4(.81,.39,.30,ribbon(tail,8.,.45)));
  result=over(result,vec4(.32,.55,.62,ribbon(tail,16.,.9)));
  float diamond=abs(q.x)/53.+abs(q.y)/70.;
  float coverage=1.-smoothstep(mix(.97,.94,uShadow),mix(1.025,1.07,uShadow),diamond);
  vec3 fabric=q.y<0.?(q.x<0.?vec3(.95,.77,.42):vec3(.88,.43,.33)):
    (q.x<0.?vec3(.94,.77,.44):vec3(.35,.57,.64));
  float grain=fract(sin(dot(floor(screen*1.7),vec2(12.9898,78.233)))*43758.5453);
  fabric*=.97+grain*.06;
  fabric=mix(fabric,vec3(.46,.40,.25),smoothstep(.93,1.0,diamond)*.78);
  float seams=1.-smoothstep(.35,.9,min(abs(q.x),abs(q.y)));
  fabric=mix(fabric,vec3(.98,.92,.71),seams*.65);
  return over(result,vec4(fabric,coverage));
}
vec4 standingBoy(vec2 p,float lod) {
  return spriteColor(textureLod(uRunSprite,(spriteUV(p)+vec2(1.))*.5,lod),p);
}
vec2 boneSource(vec2 p,vec2 sourceA,vec2 sourceB,vec2 posedA,vec2 posedB) {
  vec2 v=posedB-posedA,s=sourceB-sourceA;
  float lengthV=max(length(v),.001),lengthS=max(length(s),.001);
  vec2 axis=v/lengthV,normal=vec2(-axis.y,axis.x),sourceAxis=s/lengthS;
  return sourceA+s*(dot(p-posedA,axis)/lengthV)+vec2(-sourceAxis.y,sourceAxis.x)*dot(p-posedA,normal);
}
float rigMask(vec2 p,float side,float y0,float y1) {
  float xMask=mix(1.-smoothstep(-2.,1.,p.x),smoothstep(2.,5.,p.x),step(0.,side));
  return xMask*smoothstep(y0-.6,y0+.6,p.y)*(1.-smoothstep(y1-.6,y1+.6,p.y));
}
vec4 rigPiece(vec2 source,float side,float y0,float y1,float lod) {
  vec4 color=standingBoy(source,lod);color.a*=rigMask(source,side,y0,y1);return color;
}
vec4 rigJoint(vec2 p,vec2 center,vec3 color,float radius) {
  float d=length(p-center);
  vec4 edge=vec4(.26,.18,.12,1.-smoothstep(radius+.2,radius+.8,d));
  return over(edge,vec4(color,1.-smoothstep(radius-.6,radius,d)));
}
float runnerArmMask(vec2 p) {
  return (1.-smoothstep(-25.,-21.,p.x))*smoothstep(-8.,-3.,p.y)*(1.-smoothstep(29.,31.,p.y));
}
float runnerReelArmMask(vec2 p) {
  float boundary=p.y < -50. ? 32. : p.y < -43. ? mix(31.,34.,(p.y+50.)/7.) :
    p.y < -34. ? mix(34.,28.,(p.y+43.)/9.) : mix(28.,23.,clamp((p.y+34.)/14.,0.,1.));
  return smoothstep(boundary-.5,boundary+.5,p.x)*(1.-smoothstep(2.,6.,p.y));
}
vec4 runningBoy(vec2 p,vec4 trunk,float lod) {
  vec2 lh=vec2(-12.,37.),lk=vec2(-14.,47.),la=vec2(-15.,57.);
  vec2 rh=vec2(14.,35.),rk=vec2(18.,44.),ra=vec2(21.,51.);
  vec2 leftThigh=boneSource(p,lh,lk,uLeftHipKnee.xy,uLeftHipKnee.zw);
  vec2 leftShin=boneSource(p,lk,la,uLeftHipKnee.zw,uLeftAnkle);
  vec2 leftFoot=la+turn(p-uLeftAnkle,-uFootAngles.x);
  vec2 rightThigh=boneSource(p,rh,rk,uRightHipKnee.xy,uRightHipKnee.zw);
  vec2 rightShin=boneSource(p,rk,ra,uRightHipKnee.zw,uRightAnkle);
  vec2 rightFoot=ra+turn(p-uRightAnkle,-uFootAngles.y);
  vec4 left=rigJoint(p,uLeftHipKnee.zw,vec3(.98,.76,.47),3.5);
  left=over(left,rigJoint(p,uLeftAnkle,vec3(.97,.96,.88),2.8));
  left=over(left,rigPiece(leftThigh,-1.,35.5,49.,lod));
  left=over(left,rigPiece(leftShin,-1.,47.,58.5,lod));
  left=over(left,rigPiece(leftFoot,-1.,56.5,74.,lod));
  vec4 right=rigJoint(p,uRightHipKnee.zw,vec3(.98,.76,.47),3.5);
  right=over(right,rigJoint(p,uRightAnkle,vec3(.97,.96,.88),2.8));
  right=over(right,rigPiece(rightThigh,1.,34.5,46.,lod));
  right=over(right,rigPiece(rightShin,1.,44.,53.,lod));
  right=over(right,rigPiece(rightFoot,1.,51.5,68.,lod));
  float cut=max(rigMask(p,-1.,37.,86.),rigMask(p,1.,36.,86.));
  trunk.a*=(1.-cut)*(1.-runnerArmMask(p))*(1.-runnerReelArmMask(p));
  vec2 armSource=vec2(-23.,-5.)+turn(p-vec2(-23.,-5.),-uRig.y);
  vec4 arm=standingBoy(armSource,lod);arm.a*=runnerArmMask(armSource);
  vec2 reelPivot=vec2(22.,-7.);
  vec2 reelSource=reelPivot+turn(p-reelPivot,-uRig.z);
  vec4 reelArm=standingBoy(reelSource,lod);reelArm.a*=runnerReelArmMask(reelSource);
  vec4 legs=mix(over(left,right),over(right,left),step(uRightAnkle.y,uLeftAnkle.y));
  legs=over(legs,rigJoint(p,vec2(-23.,-5.),vec3(.87,.24,.18),4.5));
  legs=over(legs,rigJoint(p,reelPivot,vec3(.87,.24,.18),5.0));
  return over(over(over(legs,trunk),arm),reelArm);
}
vec4 characterColor(vec2 p) {
  p.x/=uMotion.z;
  vec2 boy=personSource(p),space=bodySpace(p);
  vec2 pivot=vec2(-21.,-9.);
  vec2 armSource=pivot+turn(space+uTug*.35-pivot,-(uPose.z+.18));
  float lod=uShadow*2.8;
  vec4 person=spriteColor(textureLod(uCharacter,spriteUV(boy),lod),boy);
  vec4 arm=spriteColor(textureLod(uCharacter,spriteUV(armSource),lod),armSource);
  person.a*=1.-armMask(boy);
  arm.a*=armMask(armSource);
  vec4 shoulder=rigJoint(space,pivot,vec3(.90,.22,.16),5.0);
  vec4 front=over(over(person,shoulder),arm);
  vec2 runPoint=p-vec2(0.,uMotion.w-10.);
  vec2 cell=vec2(mod(uMotion.x,2.),floor(uMotion.x*.5));
  vec4 back=spriteColor(textureLod(uRunSprite,(spriteUV(runPoint)+cell)*.5,lod),runPoint);
  back=mix(back,runningBoy(runPoint,back,lod),uRig.x);
  return mix(front,back,uMotion.y);
}
void main() {
  vec2 screen=uOffset+vec2(gl_FragCoord.x/uScale,uRes.y-gl_FragCoord.y/uScale);
  vec2 p=(screen-uPlace.xy)/uPlace.z;
  float sy=(p.y-uGround.x*p.x-uGround.y)/(uGround.x*.46-.64);
  vec2 projected=vec2(p.x+.46*sy,uGround.x*(p.x+.46*sy)+uGround.y+sy);
  vec2 personPoint=mix(p,projected,uShadow);
  vec4 person=characterColor(personPoint);
  vec4 kite=kiteColor(p-uShadow*vec2(98.,134.),screen);
  vec2 hand=uHand,attach=uKite+turn(vec2(-41.,-16.),kiteAngle());
  vec2 control=mix(hand,attach,.5)+vec2(-13.+sin(uTime*.8)*6.,18.)*(1.-uTension*.85);
  float line=1e5;vec2 previous=hand;
  for(int i=1;i<=6;i++) {
    float t=float(i)/6.;vec2 point=mix(mix(hand,control,t),mix(control,attach,t),t);
    line=min(line,segment(p,previous,point));previous=point;
  }
  vec4 result=over(kite,person);
  result=over(result,vec4(.94,.92,.77,(1.-smoothstep(.4,1.1,line))*.94));
  float contactL=(1.-smoothstep(.65,1.15,length((p-uContactL.xy)/vec2(7.,2.6))))*uContactL.z;
  float contactR=(1.-smoothstep(.65,1.15,length((p-uContactR.xy)/vec2(7.,2.6))))*uContactR.z;
  float silhouette=max(max(person.a,kite.a),max(contactL,contactR));
  o=mix(result,vec4(.12,.20,.18,silhouette*wpShadowAlpha),uShadow);
}