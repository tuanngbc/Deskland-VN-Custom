#pragma once
#include "field.h"
#include "spatial.h"
#include <queue>
namespace wallpaper {
struct Palm {
    V2 foot;int variant=0,component=0;float scale=.62f*.80f;bool mirror=false;uint64_t identity=0;
    SpriteTransform transform()const {
        // engine.rs build_palm_atlas: image dimensions and FOOT anchors, not
        // the 224x256 padded atlas quad. Original image aspect is retained.
        static constexpr float feet[]={.454f,.470f,.415f,.25f,.84f};
        float w=variant==3?148.f:variant==4?128.f:168.f,h=w*1208.f/1302.f;
        return {foot,w*scale,h*scale,feet[variant],.98f,mirror};
    }
};
struct PalmLayout {
    std::vector<Palm> trees;int requested=0,components=0,feasible=0;size_t revision=0;uint64_t nextIdentity=1;
    // Called on the wallpaper worker only, after field transition settles. No
    // per-frame search. Connected components are from the actual field texture.
    void build(const Field&field,const Geometry&geometry,const std::vector<RECT>&icons,float w,float h,int want,double time){
        auto previous=std::move(trees);trees.clear();requested=want;components=feasible=0;revision++;
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
        auto valid=[&](const Palm& p) {
            auto b=p.transform().bounds();b.x-=2;b.y-=2;b.w+=4;b.h+=4;
            if(!geometry.covers(b)||!iconClear(b,icons))return false;
            for(V2 offset:std::array<V2,7>{{{0,0},{-24,-55},{24,-55},{-55,-105},{55,-105},{-70,-18},{70,-18}}}){
                auto a=p.foot+offset*p.scale;if(field.sample(a.x,a.y,time,true)>-6)return false;
            }
            return true;
        };
        auto clear=[&](const Palm& p) {
            V2 crown=p.foot+V2{cx[p.variant]*(p.mirror?-1.f:1.f),cy[p.variant]}*p.scale;
            for(const auto& o:trees) {
                V2 other=o.foot+V2{cx[o.variant]*(o.mirror?-1.f:1.f),cy[o.variant]}*o.scale;
                if((p.foot-o.foot).length()<55||(crown-other).length()<(cr[p.variant]*p.scale+cr[o.variant]*o.scale)*.85f)return false;
            }
            return true;
        };
        for(int n=0;n<nx*ny;n++)if(labels[n]>=0){auto q=point(n);float shore=field.sample(q.x,q.y,time,true);if(shore<-180||shore>-8)continue;
            for(int v=0;v<5;v++){Palm p{q,v,labels[n],.62f*.80f*(v==4?1.35f:1.f),false};
                p.mirror=(v==1||v==4)&&(field.sample(q.x+8,q.y,time,true)-field.sample(q.x-8,q.y,time,true))*cx[v]<0;
                if(!valid(p))continue;
                float rank=abs(shore+42)*.03f+hash(n,v,7341);sites[labels[n]].push_back({p,rank});
            }
        }
        for(auto&list:sites){std::sort(list.begin(),list.end(),[](auto&a,auto&b){return a.rank<b.rank;});if(!list.empty())feasible++;}
        std::vector<int> occupied(sites.size(),0);
        std::vector<Palm> displaced;
        for(auto p:previous) {
            if(int(trees.size())>=want)break;
            const int x=int(p.foot.x/step),y=int(p.foot.y/step);
            if(x>=0&&x<nx&&y>=0&&y<ny&&labels[y*nx+x]>=0&&valid(p)&&clear(p)){
                p.component=labels[y*nx+x];trees.push_back(p);occupied[p.component]++;
            }else displaced.push_back(p);
        }
        for(auto p:displaced) {
            if(int(trees.size())>=want)break;
            Palm best=p;float distance=1e30f;
            for(const auto& island:sites) for(const auto& site:island) {
                if(site.palm.variant!=p.variant)continue;
                const float d=(site.palm.foot-p.foot).length();
                if(d>=distance)continue;
                auto candidate=p;candidate.foot=site.palm.foot;candidate.component=site.palm.component;
                if(clear(candidate)&&valid(candidate)){best=candidate;distance=d;}
            }
            if(distance<1e30f){trees.push_back(best);occupied[best.component]++;}
        }
        std::vector<size_t> cursor(sites.size(),0),order(sites.size());
        for(size_t i=0;i<order.size();++i)order[i]=i;
        bool progress=true;
        while(int(trees.size())<want&&progress){progress=false;
            // Round-robin islands: all feasible components receive their first
            // tree before any gets a second. Invalid sites are never forced.
            std::stable_sort(order.begin(),order.end(),[&](size_t a,size_t b){return occupied[a]<occupied[b];});
            for(size_t island:order){
                if(int(trees.size())>=want)break;
                auto&list=sites[island];auto&j=cursor[island];
                for(;j<list.size();j++){auto p=list[j].palm;
                    if(clear(p)){p.identity=nextIdentity++;trees.push_back(p);occupied[island]++;j++;progress=true;break;}
                }
            }
        }
        std::stable_sort(trees.begin(),trees.end(),[](auto&a,auto&b){return a.foot.y<b.foot.y;});
        std::string message="Wallpaper palms: "+std::to_string(trees.size())+"/"+std::to_string(want)+", feasible islands="+std::to_string(feasible)+"/"+std::to_string(components)+"\n";OutputDebugStringA(message.c_str());
    }
};
}
