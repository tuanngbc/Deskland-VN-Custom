// Audited visual mathematics: deterministic tile noise and Gaussian icon islands.
// New bounded storage, cancellable generation and lifetime; no legacy engine.
#pragma once
#include "gl.h"
#include "model.h"
#include <atomic>

namespace wallpaper {
inline float hash(int x,int y,int seed){uint32_t h=uint32_t(x)*374761393u^uint32_t(y)*668265263u^uint32_t(seed)*2246822519u;h=(h^(h>>13))*1274126177u;return float(double(h^(h>>16))/4294967296.);}
inline float noise(float x,float y,int seed,int period=0){int ix=int(floor(x)),iy=int(floor(y));float u=x-ix,v=y-iy;u=u*u*(3-2*u);v=v*v*(3-2*v);
    auto h=[&](int a,int b){if(period){a=(a%period+period)%period;b=(b%period+period)%period;}return hash(a,b,seed);};
    return (h(ix,iy)*(1-u)+h(ix+1,iy)*u)*(1-v)+(h(ix,iy+1)*(1-u)+h(ix+1,iy+1)*u)*v;}
inline std::vector<unsigned char> noisePixels(){std::vector<unsigned char> pixels(256*256*4);
    for(int y=0;y<256;y++)for(int x=0;x<256;x++)for(int c=0;c<4;c++){float sum=0,weight=.5f,total=0;int count=c==2?3:4;for(int i=0;i<count;i++){int period=(c==3?16:8)<<i;sum+=weight*noise(x/256.f*period,y/256.f*period,7341+100+c*50+i*13,period);total+=weight;weight*=.5f;}pixels[(y*256+x)*4+c]=static_cast<unsigned char>(std::clamp((sum/total-.5f)*1.9f+.5f,0.f,1.f)*255);}
    return pixels;
}
class Field {
    GL&g;Texture a,b;std::vector<float> from,target;int width=0,height=0;float step=5.f;double changed=0;bool transitioning=false;
public:
    explicit Field(GL&gl):g(gl){}
    float blend(double time)const{float k=float(std::clamp((time-changed)/1.8,0.,1.));return k*k*(3-2*k);}
    float sample(float x,float y,double time,bool final=false)const{
        if(target.empty())return 1000;
        // Same texel-center bilinear interpolation and clamp as fieldS in GLSL.
        float gx=std::clamp(x/step,0.f,float(width-1)),gy=std::clamp(y/step,0.f,float(height-1));int ix=int(gx),iy=int(gy),jx=std::min(ix+1,width-1),jy=std::min(iy+1,height-1);float u=gx-ix,v=gy-iy;
        auto at=[&](const std::vector<float>&a){return (a[size_t(iy)*width+ix]*(1-u)+a[size_t(iy)*width+jx]*u)*(1-v)+(a[size_t(jy)*width+ix]*(1-u)+a[size_t(jy)*width+jx]*u)*v;};
        float b=at(target);return final?b:at(from)*(1-blend(time))+b*blend(time);
    }
    bool settled(double time)const{return !transitioning||time-changed>=1.8;}
    void generate(float w,float h,const std::vector<RECT>&icons,float sigma,float warp,double time,const std::atomic<bool>&stop){
        step=std::max(5.f,std::max(w,h)/2048.f);width=int(ceil(w/step))+2;height=int(ceil(h/step))+2;
        if(uint64_t(width)*height>4*1024*1024)throw std::runtime_error("Icon field budget exceeded");
        std::vector<float> mass(size_t(width)*height,0.f);sigma=std::max(20.f,sigma);
        for(const auto&r:icons){if(stop)return;float cx=(r.left+r.right)*.5f,cy=(r.top+r.bottom)*.5f,hw=(r.right-r.left)*.25f,hh=(r.bottom-r.top)*.25f;
            float radius=5.5f*sigma+std::max(hw,hh)+warp*2;
            int x0=std::clamp(int((cx-radius)/step),0,width-1),x1=std::clamp(int((cx+radius)/step)+1,0,width-1);
            int y0=std::clamp(int((cy-radius)/step),0,height-1),y1=std::clamp(int((cy+radius)/step)+1,0,height-1);
            for(int y=y0;y<=y1;y++){if(stop)return;for(int x=x0;x<=x1;x++){float px=x*step+(noise(x*.017f,y*.017f,7342)-.47f)*warp*2.2f,py=y*step+(noise(x*.017f,y*.017f,7343)-.47f)*warp*2.2f;
                float dx=std::max(0.f,std::abs(px-cx)-hw),dy=std::max(0.f,std::abs(py-cy)-hh);mass[size_t(y)*width+x]+=exp(-(dx*dx+dy*dy)/(sigma*sigma));}}
        }
        for(auto&v:mass){float f=log(std::max(v,exp(-30.f)));v=sigma*(sqrt(std::max(-f,0.f))-sqrt(-log(.35f)))-sigma*.6f*std::max(f,0.f);}
        transitioning=from.size()==mass.size()&&!target.empty();
        if(!transitioning)from=mass;
        else{float mix=float(std::clamp((time-changed)/1.8,0.,1.));mix=mix*mix*(3-2*mix);for(size_t i=0;i<from.size();i++)from[i]+=(target[i]-from[i])*mix;}
        target=std::move(mass);changed=time;
        // R32F keeps CPU navigation/planting and the rendered shoreline on the
        // same field without half-float rounding creating a second coastline.
        a.image(g,1,width,height,0x822E,0x1903,GL_FLOAT,from.data());b.image(g,2,width,height,0x822E,0x1903,GL_FLOAT,target.data());
    }
    void bind(Program&p,double time){a.bind(g,1);b.bind(g,2);p.i("uFA",1);p.i("uFB",2);p.v2("uGrid",float(width),float(height));p.f("uStep",step);float k=float(std::clamp((time-changed)/1.8,0.,1.));p.f("uMix",k*k*(3-2*k));}
};
// Spatial bins contain at most 36 circles per cell, matching the existing assets.
class Clouds {
    GL&g;Texture texture;std::vector<float> bins;int columns=0,rows=0;double last=-100;
    struct Puff {float x=0,y=0;double born=-100;};std::array<Puff,4> puffs{};size_t nextPuff=0;
public:
    explicit Clouds(GL&gl):g(gl){}
    void burst(float x,float y,double time){puffs[nextPuff++%puffs.size()]={x,y,time};last=-100;}
    void update(float w,float h,float cover,float flow,double time){if(time-last<.10)return;last=time;
        columns=int(ceil(w/48))+8;rows=int(ceil(h/48))+8;
        if(columns*36>16384||uint64_t(columns)*rows*36>2*1024*1024)throw std::runtime_error("Cloud field budget exceeded");
        bins.assign(size_t(columns)*rows*36*4,0);std::vector<unsigned char> count(size_t(columns)*rows,0);
        const int n=std::clamp(int(w*h/180000*cover),0,48);
        for(int i=0;i<n+4;i++){float x=0,y=0,life=1;
            if(i<n){x=fmod(hash(i,1,81)*(w+500)+float(time)*flow*(7+hash(i,2,92)*10),w+500)-250;y=h*(.12f+.66f*hash(i,3,81));}
            else {auto&p=puffs[size_t(i-n)];double age=time-p.born;if(age<0||age>7)continue;life=float(std::min(age/.35,1.)*std::min((7-age)/2,1.));x=p.x+float(age)*flow*8;y=p.y-float(age)*3;}
            for(int j=0;j<5;j++){float r=(38+hash(i,j,71)*42)*life,cx=x+(j-2)*42*life,cy=y-sin(j*1.7f)*18*life;
                int x0=std::clamp(int(floor((cx-r)/48))+4,0,columns-1),x1=std::clamp(int(floor((cx+r)/48))+4,0,columns-1),y0=std::clamp(int(floor((cy-r)/48))+4,0,rows-1),y1=std::clamp(int(floor((cy+r)/48))+4,0,rows-1);
                for(int gy=y0;gy<=y1;gy++)for(int gx=x0;gx<=x1;gx++){size_t cell=size_t(gy)*columns+gx;if(count[cell]>=36)continue;size_t k=(cell*36+count[cell]++)*4;bins[k]=cx;bins[k+1]=cy;bins[k+2]=r;bins[k+3]=hash(i,j,91);}
            }
        }texture.image(g,3,columns*36,rows,0x8814,GL_RGBA,GL_FLOAT,bins.data());
    }
    void bind(Program&p){texture.bind(g,3);p.i("uCirc",3);p.v2("uCGrid",float(columns),float(rows));p.f("uCellG",48);}
#ifdef WALLPAPER_HOSTED_PROBE
    void probeDisc(float w,float h,float centerX,float centerY,float radius){
        columns=int(ceil(w/48))+8;rows=int(ceil(h/48))+8;bins.assign(size_t(columns)*rows*36*4,0);
        for(int cell=0;cell<columns*rows;cell++){size_t at=size_t(cell)*36*4;bins[at]=centerX;bins[at+1]=centerY;bins[at+2]=radius;}
        texture.image(g,3,columns*36,rows,0x8814,GL_RGBA,GL_FLOAT,bins.data());
    }
#endif
};
}
