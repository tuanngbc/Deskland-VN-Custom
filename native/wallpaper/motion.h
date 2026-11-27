// Bounded fixed-step steering, audited against legacy turn_toward, look-ahead
// shore avoidance and predictive actor separation. No object timers/threads.
#pragma once
#include "spatial.h"
namespace wallpaper {
struct MotionSpec {float speed,accel,brake,turn,angularAccel,radius;bool airborne=false;};
struct Body {
    V2 position,velocity,target;float heading=0,angularVelocity=0,phase=0,idle=0;
    bool initialized=false,command=false;
    void aim(V2 p){target=p;command=true;idle=0;} // never resets physical state
    float speed()const{return velocity.length();}
};
template<class Valid>inline void steer(Body&b,const MotionSpec&s,V2 destination,float dt,Valid valid,V2 separation={}){
    V2 delta=destination-b.position;float distance=delta.length();V2 desired=unit(delta)+separation;
    const float speed=b.speed(),look=s.radius+speed*speed/(2*s.brake)+speed*.65f;
    // Choose a navigable heading using a forward corridor, not a new position.
    auto corridor=[&](float a,float reach){for(float f:{.25f,.5f,.75f,1.f})if(!valid(b.position+V2{cos(a),sin(a)}*(reach*f),s.radius))return false;return true;};
    float requested=atan2(desired.y,desired.x);bool avoiding=false;
    if(!corridor(b.heading,look)||!corridor(requested,look)){
        avoiding=true;float best=-1e9f,chosen=b.heading;
        for(int i=-8;i<=8;i++){float a=b.heading+i*.19634954f;if(!corridor(a,look))continue;
            float score=cos(angleDelta(a,requested))-.08f*abs(float(i));if(score>best){best=score;chosen=a;}}
        if(best<-1e8f){ // braking distance unavailable: search a shorter escape arc
            for(int i=-16;i<=16;i++){float a=b.heading+i*.19634954f;if(corridor(a,s.radius+3)){float score=cos(angleDelta(a,requested));if(score>best){best=score;chosen=a;}}}
        }requested=chosen;
    }
    float error=angleDelta(b.heading,requested),angularTarget=std::clamp(error*3,-s.turn,s.turn);
    b.angularVelocity+=std::clamp(angularTarget-b.angularVelocity,-s.angularAccel*dt,s.angularAccel*dt);
    b.heading+=b.angularVelocity*dt;
    float desiredSpeed=s.speed;
    if(b.command&&!s.airborne)desiredSpeed=std::min(desiredSpeed,sqrt(std::max(0.f,2*s.brake*(distance-2))));
    desiredSpeed*=std::clamp(cos(error),s.airborne?.68f:.08f,1.f);
    // Slow before an obstacle while retaining momentum and bounded acceleration.
    if(avoiding&&!corridor(b.heading,look))desiredSpeed=s.airborne?s.speed*.68f:0;
    V2 goal=V2{cos(b.heading),sin(b.heading)}*desiredSpeed;
    V2 dv=goal-b.velocity;float limit=(desiredSpeed<speed?s.brake:s.accel)*dt;
    if(dv.length()>limit)dv=unit(dv)*limit;
    V2 nextVelocity=b.velocity+dv,next=b.position+(b.velocity+nextVelocity)*(.5f*dt);
    // A final swept guard prevents penetration after topology/shore edits. Normal
    // navigation brakes upstream; emergency contact removes only normal motion.
    if(valid(next,s.radius)){b.position=next;b.velocity=nextVelocity;}
    else {b.velocity=b.velocity+unit(b.velocity)*(-std::min(speed,s.brake*dt));}
    b.phase+=b.speed()*dt/std::max(4.f,s.radius*.7f);b.idle+=dt;
    if(b.command&&distance<4&&b.speed()<2&&!s.airborne){b.command=false;b.idle=-3;}
}
inline V2 separate(const Body&b,const Body*others,size_t count,float radius){V2 force{};
    for(size_t i=0;i<count;i++){const auto&o=others[i];if(!o.initialized||&o==&b)continue;V2 d=b.position-o.position;float n=d.length();
        V2 future=d+(b.velocity-o.velocity)*.8f;float closest=std::min(n,future.length());
        if(closest<radius&&n>.001f)force=force+unit(d)*(2*(radius-closest)/radius);
    }return force;
}
}
