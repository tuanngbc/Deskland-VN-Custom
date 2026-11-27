#pragma once
#include "field.h"
#include "spatial.h"
#include <queue>
namespace wallpaper {
struct Palm {
    V2 foot;int variant=0,component=0;float scale=.62f*.80f;bool mirror=false;
    SpriteTransform transform()const {
        // engine.rs build_palm_atlas: image dimensions and FOOT anchors, not
        // the 224x256 padded atlas quad. Original image aspect is retained.
        static constexpr float feet[]={.454f,.470f,.415f,.25f,.84f};
        float w=variant==3?148.f:variant==4?128.f:168.f,h=w*1208.f/1302.f;
        return {foot,w*scale,h*scale,feet[variant],.98f,mirror};
    }
};
struct PalmLayout {
    std::vector<Palm> trees;int requested=0,components=0,feasible=0;size_t revision=0;
    // Called on the wallpaper worker only, after field transition settles. No
    // per-frame search. Connected components are from the actual field texture.
    void build(const Field&field,const Geometry&geometry,const std::vector<RECT>&icons,float w,float h,int want,double time){
        trees.clear();requested=want;components=feasible=0;revision++;
        const float step=13;int nx=int(ceil(w/step)),ny=int(ceil(h/step));
        if(uint64_t(nx)*ny>1024*1024)throw std::runtime_error("Palm component budget exceeded");
        std::vector<int> labels(size_t(nx)*ny,-1);auto point=[&](int n){return V2{(n%nx+.5f)*step,(n/nx+.5f)*step};};
        for(int n=0;n<nx*ny;n++){auto p=point(n);if(geometry.contains(p)&&field.sample(p.x,p.y,time,true)<-2)labels[n]=-2;}
        std::queue<int> queue;
        for(int n=0;n<nx*ny;n++)if(labels[n]==-2){labels[n]=components;queue.push(n);
            while(!queue.empty()){int k=queue.front();queue.pop();for(int d:std::array<int,4>{-1,1,-nx,nx}){int q=k+d;if(q<0||q>=nx*ny||(abs(d)==1&&q/nx!=k/nx)||labels[q]!=-2)continue;labels[q]=components;queue.push(q);}}
            components++;
        }
        struct Candidate {Palm palm;float rank;};std::vector<std::vector<Candidate>> sites(components);
        static constexpr float cx[]={9,18,27,40,-45},cy[]={-84,-80,-82,-78,-65},cr[]={80,84,84,78,70};
        for(int n=0;n<nx*ny;n++)if(labels[n]>=0){auto q=point(n);float shore=field.sample(q.x,q.y,time,true);if(shore<-180||shore>-8)continue;
            for(int v=0;v<5;v++){Palm p{q,v,labels[n],.62f*.80f*(v==4?1.35f:1.f),false};
                p.mirror=(v==1||v==4)&&(field.sample(q.x+8,q.y,time,true)-field.sample(q.x-8,q.y,time,true))*cx[v]<0;
                auto b=p.transform().bounds();b.x-=2;b.y-=2;b.w+=4;b.h+=4;
                if(!geometry.covers(b)||!iconClear(b,icons))continue;
                bool sand=true;for(V2 offset:std::array<V2,7>{{{0,0},{-24,-55},{24,-55},{-55,-105},{55,-105},{-70,-18},{70,-18}}}){
                    auto a=q+offset*p.scale;if(field.sample(a.x,a.y,time,true)>-6){sand=false;break;}}
                if(!sand)continue;
                float rank=abs(shore+42)*.03f+hash(n,v,7341);sites[labels[n]].push_back({p,rank});
            }
        }
        for(auto&list:sites){std::sort(list.begin(),list.end(),[](auto&a,auto&b){return a.rank<b.rank;});if(!list.empty())feasible++;}
        std::vector<size_t> cursor(sites.size(),0);bool progress=true;
        while(int(trees.size())<want&&progress){progress=false;
            // Round-robin islands: all feasible components receive their first
            // tree before any gets a second. Invalid sites are never forced.
            for(size_t island=0;island<sites.size()&&int(trees.size())<want;island++){
                auto&list=sites[island];auto&j=cursor[island];
                for(;j<list.size();j++){auto p=list[j].palm;bool clear=true;
                    V2 crown=p.foot+V2{cx[p.variant]*(p.mirror?-1.f:1.f),cy[p.variant]}*p.scale;
                    for(auto&o:trees){V2 other=o.foot+V2{cx[o.variant]*(o.mirror?-1.f:1.f),cy[o.variant]}*o.scale;
                        if((p.foot-o.foot).length()<55||(crown-other).length()<(cr[p.variant]*p.scale+cr[o.variant]*o.scale)*.85f){clear=false;break;}}
                    if(clear){trees.push_back(p);j++;progress=true;break;}
                }
            }
        }
        std::stable_sort(trees.begin(),trees.end(),[](auto&a,auto&b){return a.foot.y<b.foot.y;});
        std::string message="Wallpaper palms: "+std::to_string(trees.size())+"/"+std::to_string(want)+", feasible islands="+std::to_string(feasible)+"/"+std::to_string(components)+"\n";OutputDebugStringA(message.c_str());
    }
};
}
