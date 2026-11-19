#version 330 core
layout(location=0) in vec3 aPosition;
layout(location=1) in vec3 aNormal;
layout(location=2) in vec2 aPart;
uniform vec2 uRes;
uniform vec4 uWhale;
uniform vec4 uPose; // centre height, pitch, roll, arch
uniform vec4 uStroke; // continuous tail phase, left/right fin angle, surface
uniform vec2 uDetail; // visibility, juvenile proportions
uniform float uEdgeAA;
out vec3 vNormal;
out vec3 vSkin;
out float vHeight;
out float vPart;
out float vBelly;
out float vFacing;

mat2 rotation(float a) { float c=cos(a),s=sin(a);return mat2(c,s,-s,c); }
void main() {
    vec3 p=aPosition,n=aNormal;
    p.y*=1.0+uDetail.y*0.05*smoothstep(-20.0,30.0,p.x);
    // Pectoral fins flex from their roots, with independent slow recovery.
    if(aPart.x>0.5&&aPart.x<1.5) {
        float reach=clamp((abs(p.y)-13.5)/42.0,0.0,1.0);
        float angle=(aPart.y<0.0?uStroke.y:uStroke.z)*aPart.y;
        p.yz-=vec2(aPart.y*13.5,-3.0);
        p.yz=rotation(angle*(0.3+reach*0.7))*p.yz;
        p.yz+=vec2(aPart.y*13.5,-3.0);
        n.yz=rotation(angle*(0.3+reach*0.7))*n.yz;
    }
    // A travelling vertical bend grows along the peduncle. Flukes change pitch
    // on each power/recovery stroke instead of sliding left and right.
    float tail=1.0-smoothstep(-72.0,4.0,p.x);
    float beat=sin(uStroke.x-tail*0.75);
    if(aPart.x>1.5&&aPart.x<2.5) {
        float fluke=sin(uStroke.x-0.9)*0.29;
        p.x+=70.0;p.xz=rotation(fluke)*p.xz;p.x-=70.0;
        n.xz=rotation(fluke)*n.xz;
    }
    p.z+=beat*tail*tail*11.0;
    n.xz=rotation(beat*tail*0.20)*n.xz;
    p.z+=uPose.w*sin(clamp((p.x+73.0)/138.0,0.0,1.0)*3.14159)*5.5;
    // Pitch rotates head and tail together through the actual water plane.
    p.yz=rotation(uPose.z)*p.yz;n.yz=rotation(uPose.z)*n.yz;
    p.xz=rotation(uPose.y)*p.xz;n.xz=rotation(uPose.y)*n.xz;
    p.z+=uPose.x;
    vec2 planar=rotation(uWhale.z)*p.xy;
    vec2 scene=uWhale.xy+(planar-vec2(0.0,p.z*0.62))*uWhale.w;
    n.xy=rotation(uWhale.z)*n.xy;
    float facing=dot(normalize(n),normalize(vec3(0.0,0.62,1.0)));
    // Subpixel silhouette fringe, local to the animal. It needs neither a
    // multisample target nor a framebuffer resolve/copy per whale per frame.
    vec2 outward=vec2(n.x,n.y-0.62*n.z);
    scene+=outward/max(length(outward),0.001)*uEdgeAA*(1.0-smoothstep(0.0,0.45,abs(facing)));
    gl_Position=vec4(scene.x/uRes.x*2.0-1.0,1.0-scene.y/uRes.y*2.0,0.0,1.0);
    vNormal=normalize(n);vSkin=aPosition;vHeight=p.z;vPart=aPart.x;
    vBelly=1.0-smoothstep(-0.55,0.05,aNormal.z);
    vFacing=facing;
}
