// Small bounded A* planner. Runs only for a new water waypoint on the existing
// wallpaper worker; the steering integrator remains responsible for movement.
#pragma once
#include "motion.h"
#include <queue>
#include <limits>
namespace wallpaper {
template<class Valid>std::vector<V2> waterRoute(V2 start,V2 target,float w,float h,float radius,Valid valid){
    auto line=[&](V2 a,V2 b){int n=std::max(1,int(ceil((b-a).length()/8)));for(int i=0;i<=n;i++)if(!valid(a+(b-a)*(float(i)/n),radius))return false;return true;};
    if(line(start,target))return {target};
    float step=std::max(32.f,std::max(w,h)/128.f);int nx=int(ceil(w/step)),ny=int(ceil(h/step)),count=nx*ny;if(count>32768)return {};
    auto point=[&](int n){return V2{(n%nx+.5f)*step,(n/nx+.5f)*step};};
    std::vector<signed char> pass(count,-1);auto cellValid=[&](int n){if(pass[n]<0)pass[n]=valid(point(n),radius)?1:0;return pass[n]==1;};
    auto nearestCell=[&](V2 q){int best=-1;float distance=1e9;int cx=int(q.x/step),cy=int(q.y/step);
        for(int y=std::max(0,cy-3);y<std::min(ny,cy+4);y++)for(int x=std::max(0,cx-3);x<std::min(nx,cx+4);x++){int n=y*nx+x;float d=(point(n)-q).length();if(d<distance&&cellValid(n)&&line(q,point(n))){best=n;distance=d;}}return best;};
    int source=nearestCell(start),goal=nearestCell(target);if(source<0||goal<0)return {};
    struct Node{float cost;int cell;bool operator<(const Node&b)const{return cost>b.cost;}};
    std::priority_queue<Node> open;std::vector<float> cost(count,std::numeric_limits<float>::infinity());std::vector<int> parent(count,-1);cost[source]=0;open.push({0,source});
    int visits=0;while(!open.empty()&&visits++<count*4){int n=open.top().cell;open.pop();if(n==goal)break;
        for(int dy=-1;dy<=1;dy++)for(int dx=-1;dx<=1;dx++){if(!dx&&!dy)continue;int x=n%nx+dx,y=n/nx+dy;if(x<0||x>=nx||y<0||y>=ny)continue;int q=y*nx+x;if(!cellValid(q))continue;
            if(dx&&dy&&(!cellValid(n+dx)||!cellValid(n+dy*nx)))continue;
            float next=cost[n]+(dx&&dy?1.414214f:1.f);if(next>=cost[q]||!line(point(n),point(q)))continue;cost[q]=next;parent[q]=n;open.push({next+(point(q)-point(goal)).length()/step,q});}
    }
    if(source!=goal&&parent[goal]<0)return {};
    std::vector<V2> raw{target};for(int n=goal;n!=source;n=parent[n]){if(n<0||raw.size()>512)return {};raw.push_back(point(n));}raw.push_back(point(source));std::reverse(raw.begin(),raw.end());
    std::vector<V2> route;V2 from=start;for(size_t i=0;i<raw.size();){size_t last=i;for(size_t j=i+1;j<raw.size();j++){if(!line(from,raw[j]))break;last=j;}route.push_back(raw[last]);from=raw[last];i=last+1;if(route.size()>128)return {};}
    return route;
}
}
