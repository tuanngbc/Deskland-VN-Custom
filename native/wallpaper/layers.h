// Explicit wallpaper composition, audited against the original artwork passes.
// Physical ground is never inferred from the colour of the topmost cloud pixel.
#pragma once
#include <string>
#include <stdexcept>
namespace wallpaper {
inline std::string transparentMixes(std::string text){
    const std::string start="col = mix(col,";size_t at=0;
    while((at=text.find(start,at))!=std::string::npos){size_t value=at+start.size(),comma=std::string::npos,end=std::string::npos;int depth=0;
        for(size_t i=value;i<text.size();i++){char c=text[i];if(c=='(')depth++;else if(c==')'){if(!depth){end=i;break;}depth--;}else if(c==','&&!depth)comma=i;}
        if(comma==std::string::npos||end==std::string::npos)throw std::runtime_error("Invalid artwork layer mix");
        std::string replacement="wpOver(col,wpAlpha,"+text.substr(value,comma-value)+","+text.substr(comma+1,end-comma-1)+")";
        text.replace(at,end-at+1,replacement);at+=replacement.size();
    }return text;
}
inline std::string meadowComposition(std::string text){
    auto main=text.find("void main()"),begin=text.find('{',main)+1;
    auto shadow=text.find("  // ---------- Bóng mây đổ trên đồng cỏ",begin),cloud=text.find("  // ---------- Mây ----------",shadow),end=text.find("  if (uDebug",cloud);
    if(main==std::string::npos||shadow==std::string::npos||cloud==std::string::npos||end==std::string::npos)throw std::runtime_error("Meadow composition reference changed");
    const std::string over=R"(
uniform float wpCompositionPass;
void wpOver(inout vec3 c,inout float alpha,vec3 colour,float coverage){
  coverage=clamp(coverage,0.,1.);float a=coverage+alpha*(1.-coverage);
  c=(colour*coverage+c*alpha*(1.-coverage))/max(a,.00001);alpha=a;
}
void main(){
 if(wpCompositionPass<.5){
)";
    // Pass 0: grass/ground marks. Actors and grounded shadows are then drawn on
    // the framebuffer. Pass 1: cloud shadow multiplication. Pass 2: sky clouds
    // occlude the actors, followed by the original airborne dandelion layer.
    return text.substr(0,main)+over+text.substr(begin,shadow-begin)+R"(
   o=vec4(col,1.);return;
 }
 vec2 p=vec2(gl_FragCoord.x,uRes.y*uScale-gl_FragCoord.y)/uScale;
 if(wpCompositionPass<1.5){float coverage=ss(3.,-3.,cloudDist(p-SHADOW_OFF))*.85;
   o=vec4(mix(vec3(1.),SHADOW,coverage),1.);return;}
 vec3 col=vec3(0.);float wpAlpha=0.;
)"+transparentMixes(text.substr(cloud,end-cloud))+"\n o=vec4(col,wpAlpha);\n}\n";
}
inline std::string beachAirComposition(std::string text){
    const std::string condition="if (dot(d0, d0) < 400.0)";size_t at=text.find(condition);
    if(at==std::string::npos)throw std::runtime_error("Beach air artwork contract changed");
    // Bird shadows remain in the terrain shader; visible birds are drawn above
    // palm canopies. Kayaks, fish, turtles and their water layers stay intact.
    text.replace(at,condition.size(),"if (false)");
    auto main=text.find("void main()");text.insert(main,"uniform float wpAirPass;\n");main=text.find("void main()");auto begin=text.find('{',main)+1;
    text.insert(begin,R"(
 if(wpAirPass>.5){
   vec2 p=vec2(gl_FragCoord.x,uRes.y*uScale-gl_FragCoord.y)/uScale;
   vec3 c=vec3(0.);float alpha=0.;
   for(int i=0;i<8;i++){if(float(i)>=uBirdN)break;vec4 b=uBird[i];vec2 delta=p-b.xy;
     if(dot(delta,delta)>=400.)continue;vec3 g=gullSD(rot(-b.z)*delta,b.w);
     vec3 wing=mix(vec3(.86,.90,.94),vec3(.22,.25,.30),ss(.72,.8,g.z));
     float a=ss(.6,-.6,g.y);c=wing*a+c*(1.-a);alpha=a+alpha*(1.-a);
     float body=ss(.6,-.6,g.x);c=vec3(.99)*body+c*(1.-body);alpha=body+alpha*(1.-body);
   }o=vec4(c/max(alpha,.00001),alpha);return;
 }
)");return text;
}
inline std::string catHillDepthComposition(std::string text){
    auto first=text.find("  // ---------- 5. Đá cuội"),last=text.find("  // ---------- 7. Tán cây",first);
    if(first==std::string::npos||last==std::string::npos)throw std::runtime_error("Cat Hill depth artwork contract changed");
    text.replace(first,last-first,R"(
  float swayTree = sin(t * 1.6 * uWind) * 3.5 * s * uWind + sin(t * 20.0) * 4.5 * s * uTreeShake;
  const int order[12]=int[12](0,5,4,1,7,6,3,2,9,11,10,8);
  const float depth[12]=float[12](.45,.45,.46,.47,.54,.56,.57,.63,.63,.64,.66,.69);
  int scenery=0,cat=0;bool toyDrawn=uToy.w<=0.;
  for(int layer=0;layer<28;layer++){
    float sy=scenery<12?depth[scenery]*H:1.e20;
    float cy=float(cat)<uCatN?uCatPos[cat].y:1.e20;
    float yy=toyDrawn?1.e20:uToy.y;
    if(min(min(sy,cy),yy)>1.e19)break;
    if(sy<=cy&&sy<=yy){int item=order[scenery++];
      if(item==0)drawRock(col,p,vec2(W*.13,H*.45),.95*s);
      else if(item==1)drawRock(col,p,vec2(W*.88,H*.47),.90*s);
      else if(item==2)drawRock(col,p,vec2(W*.43,H*.63),.85*s);
      else if(item==3)drawCushion(col,p,vec2(W*.26,H*.57),1.05*s);
      else if(item==4)drawLeafCluster(col,p,vec2(W*.36+swayTree*.4,H*.46),19.*s,1);
      else if(item==5)drawLeafCluster(col,p,vec2(W*.64+swayTree*.4,H*.45),21.*s,0);
      else if(item==6)drawLeafCluster(col,p,vec2(W*.48+swayTree*.5,H*.56),18.*s,2);
      else if(item==7)drawLeafCluster(col,p,vec2(W*.74+swayTree*.5,H*.54),22.*s,1);
      else if(item==8)drawMidTree(col,p,vec2(W*.18,H*.69),.95*s,swayTree);
      else if(item==9)drawMidTree(col,p,vec2(W*.32,H*.63),1.05*s,-swayTree*.9);
      else if(item==10)drawMidTree(col,p,vec2(W*.72,H*.66),s,swayTree*.85);
      else drawMidTree(col,p,vec2(W*.86,H*.64),.98*s,-swayTree);
    }else if(cy<=yy){drawCat(col,p,uCatPos[cat],uCatMeta[cat]);cat++;}
    else {drawYarnBall(col,p,uToy,s);toyDrawn=true;}
  }
)");return text;
}
}
