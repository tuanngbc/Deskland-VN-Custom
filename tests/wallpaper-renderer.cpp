// Actual shader/scene execution on a hosted software GL device, never owner PC.
#define WALLPAPER_HOSTED_PROBE
#include "../native/wallpaper/session.h"
#include <iostream>
#include <set>
#include <filesystem>

static void check(bool b,const char*message){if(!b)throw std::runtime_error(message);}
static void png(const std::filesystem::path&file,int w,int h,const std::vector<unsigned char>&bottomUp){
    using namespace wallpaper;std::vector<unsigned char> top(bottomUp.size());for(int y=0;y<h;y++)memcpy(top.data()+size_t(y)*w*4,bottomUp.data()+size_t(h-y-1)*w*4,size_t(w)*4);
    Com<IWICImagingFactory> factory;Com<IWICStream> stream;Com<IWICBitmapEncoder> encoder;Com<IWICBitmapFrameEncode> frame;Com<IPropertyBag2> properties;
    check(SUCCEEDED(CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(factory.put()))),"PNG factory");
    check(SUCCEEDED(factory->CreateStream(stream.put()))&&SUCCEEDED(stream->InitializeFromFilename(file.c_str(),GENERIC_WRITE)),"PNG stream");
    check(SUCCEEDED(factory->CreateEncoder(GUID_ContainerFormatPng,nullptr,encoder.put()))&&SUCCEEDED(encoder->Initialize(stream.get(),WICBitmapEncoderNoCache)),"PNG encoder");
    check(SUCCEEDED(encoder->CreateNewFrame(frame.put(),properties.put()))&&SUCCEEDED(frame->Initialize(properties.get()))&&SUCCEEDED(frame->SetSize(w,h)),"PNG frame");
    WICPixelFormatGUID format=GUID_WICPixelFormat32bppBGRA;check(SUCCEEDED(frame->SetPixelFormat(&format))&&IsEqualGUID(format,GUID_WICPixelFormat32bppBGRA),"PNG BGRA pixel format");
    for(size_t i=0;i<top.size();i+=4)std::swap(top[i],top[i+2]);
    check(SUCCEEDED(frame->WritePixels(h,w*4,UINT(top.size()),top.data()))&&SUCCEEDED(frame->Commit())&&SUCCEEDED(encoder->Commit()),"PNG write");
}
int main(int argc,char**argv){try{
    wchar_t hosted[16]{};GetEnvironmentVariableW(L"GITHUB_ACTIONS",hosted,16);check(!wcscmp(hosted,L"true"),"Hosted verification only");check(argc==2,"Evidence path required");
    using namespace wallpaper;CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    std::filesystem::path evidence=argv[1];std::filesystem::create_directories(evidence);std::filesystem::create_directories(assetsRoot()/L"wallpaper/thumbs");
    const auto patch=evidence/"color-contract.png";const std::vector<unsigned char> red{255,0,0,255};png(patch,1,1,red);check(decode(patch).rgba==red,"PNG color channels changed");
    std::ofstream report(evidence/"renderer.txt");Window hidden;Context context(hidden.h);GL gl;std::cout<<"GL_VERSION="<<glGetString(GL_VERSION)<<'\n';std::cout<<"GL_RENDERER="<<glGetString(GL_RENDERER)<<'\n';
    GLuint vao=0,fbo=0;gl.GenVertexArrays(1,&vao);gl.BindVertexArray(vao);gl.GenFramebuffers(1,&fbo);
    Texture target,noiseTexture;auto noise=noisePixels();noiseTexture.image(gl,0,256,256,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,noise.data(),true);
    auto allocate=[&](int w,int h){target.image(gl,4,w,h,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,nullptr);gl.BindFramebuffer(0x8D40,fbo);gl.FramebufferTexture2D(0x8D40,0x8CE0,GL_TEXTURE_2D,target.name(),0);check(gl.CheckFramebufferStatus(0x8D40)==0x8CD5,"Probe surface incomplete");glViewport(0,0,w,h);};
    auto capture=[&](int w,int h){std::vector<unsigned char> bytes(size_t(w)*h*4);glReadPixels(0,0,w,h,GL_RGBA,GL_UNSIGNED_BYTE,bytes.data());check(glGetError()==GL_NO_ERROR,"Render/readback error");return bytes;};
    Config config;config.clockLighting=false;config.manualHour=12;Weather weather;std::atomic<bool> stop{false};const std::vector<RECT> icons{{24,65,93,143},{24,170,93,248},{135,65,204,143}};
    View view{0,0,640,360,640,360};
    std::vector<unsigned char> sheet(size_t(2560)*1440*4,255);
    for(int i=0;i<int(catalog().size());i++){
        allocate(640,360);noiseTexture.bind(gl,0);SceneRuntime scene(gl,i);scene.render(config,view,icons,weather,2.,.04f,stop);auto before=capture(640,360);
        noiseTexture.bind(gl,0);scene.render(config,view,icons,weather,2.,0,stop);check(capture(640,360)==before,"First texture load differs from same-time render");
        std::set<uint32_t> colors;for(size_t p=0;p<before.size();p+=16){uint32_t value;memcpy(&value,before.data()+p,4);colors.insert(value);}check(colors.size()>30,"Scene is blank/constant");
        png(evidence/(std::string(catalog()[i].id)+".png"),640,360,before);png(assetsRoot()/L"wallpaper/thumbs"/(std::string(catalog()[i].id)+".png"),640,360,before);
        // Unmodified renderer pixels, ordered row-major exactly as catalog.
        for(int row=0;row<360;row++)memcpy(sheet.data()+(size_t((3-i/4)*360+row)*2560+(i%4)*640)*4,before.data()+size_t(row)*640*4,640*4);
        noiseTexture.bind(gl,0);scene.render(config,view,icons,weather,2.12,.12f,stop);auto animated=capture(640,360);check(before!=animated,"Scene never animates");
        // Two identically seeded, identically advanced worlds differ only by input.
        SceneRuntime clicked(gl,i);noiseTexture.bind(gl,0);clicked.render(config,view,icons,weather,2.,.04f,stop);
        auto bodies=clicked.actors();auto boy=clicked.meadowBoy();auto fish=clicked.fish();clicked.click(320,240);
        for(auto&[name,list]:bodies)for(size_t j=0;j<list.size();j++){auto&a=list[j];auto&b=clicked.actors().at(name)[j];check((a.position-b.position).length()==0&&(a.velocity-b.velocity).length()==0&&a.heading==b.heading,"Input teleported an actor");}
        check((boy.position-clicked.meadowBoy().position).length()==0,"Input teleported meadow boy");for(size_t j=0;j<fish.size();j++)check(fish[j].x==clicked.fish()[j].x&&fish[j].y==clicked.fish()[j].y,"Input teleported fishing actor");
        noiseTexture.bind(gl,0);clicked.render(config,view,icons,weather,2.12,.12f,stop);auto after=capture(640,360);
        if(animated==after)throw std::runtime_error(std::string(catalog()[i].id)+" has no verified click response");
        png(evidence/(std::string(catalog()[i].id)+"-click.png"),640,360,after);
        report<<catalog()[i].id<<"=RENDER_ANIMATION_AND_ISOLATED_CLICK_PASS\n";std::cout<<catalog()[i].id<<" PASS\n";
    }
    png(evidence/"all-16-contact-sheet.png",2560,1440,sheet);report<<"CATALOG_ORDER=meadow,beach,beach-turquoise,lake,sky,ocean,rainforest,mocchau,tet-sakura,cat-hill,stadium,aquarium,commute,summer-porch,cats,boxing-ring\nINPUT_PRESERVES_ALL_PHYSICAL_STATE=PASS\n";
    {
        // Force an actual cloud over a ground actor, and compare the exact
        // same renderer with the actor hidden. A random contact sheet missed
        // the previous semantic inversion, so overlap is mandatory here.
        SceneRuntime meadow(gl,sceneIndex("meadow"));View v{0,0,960,540,960,540};allocate(960,540);Config cfg=config;
        cfg.sceneParams["meadow"]["cover"]=0;cfg.sceneParams["meadow"]["showCat"]=0;
        auto draw=[&](){noiseTexture.bind(gl,0);meadow.render(cfg,v,{},weather,30,0,stop);return capture(960,540);};
        auto visible=draw();auto b=meadow.boyBounds();check(meadow.meadowBoy().initialized,"Meadow fixture has no boy");
        cfg.sceneParams["meadow"]["showBoy"]=0;auto grass=draw();int changed=0;
        auto equalPixel=[](auto&a,auto&b,size_t at){return std::equal(a.begin()+at,a.begin()+at+3,b.begin()+at);};
        for(int y=int(b.y);y<int(b.y+b.h);y++)for(int x=int(b.x);x<int(b.x+b.w);x++){size_t at=(size_t(539-y)*960+x)*4;if(!equalPixel(visible,grass,at))changed++;}
        check(changed>8,"Ground actor absent from uncovered grass");
        meadow.probeCloud(meadow.meadowBoy().position,120);auto cloudOnly=draw();cfg.sceneParams["meadow"]["showBoy"]=1;auto covered=draw();
        for(int y=int(b.y);y<int(b.y+b.h);y++)for(int x=int(b.x);x<int(b.x+b.w);x++){size_t at=(size_t(539-y)*960+x)*4;check(equalPixel(covered,cloudOnly,at),"Ground boy painted on top of cloud");}
        png(evidence/"meadow-ground-uncovered.png",960,540,visible);png(evidence/"meadow-cloud-occludes-boy.png",960,540,covered);
        report<<"MEADOW_CONTROLLED_CLOUD_OVER_GROUND_ACTOR=PASS\n";
    }
    {
        SceneRuntime beach(gl,sceneIndex("beach"));allocate(640,360);Config cfg=config;cfg.sceneParams["beach"]["birds"]=1;cfg.sceneParams["beach"]["palms"]=0;
        std::vector<RECT> island{{260,280,340,330}};
        auto draw=[&](){noiseTexture.bind(gl,0);beach.render(cfg,view,island,weather,30,0,stop);return capture(640,360);};
        draw();beach.probeActor("uBird",0,{300,185});beach.probePalms({});auto bird=draw();
        // Anchor on known icon-generated sand, with canopy directly under bird.
        beach.probePalms({Palm{{300,235},0,0,.496f,false}});check(beach.shore({300,235})<-6,"Palm overlap fixture is not sand");auto overlap=draw();cfg.sceneParams["beach"]["birds"]=0;auto palmOnly=draw();int matched=0,canopy=0;
        for(int y=180;y<=190;y++)for(int x=293;x<=307;x++){size_t at=(size_t(359-y)*640+x)*4;
            if(bird[at]>240&&bird[at+1]>240&&bird[at+2]>240&&std::equal(bird.begin()+at,bird.begin()+at+3,overlap.begin()+at)){matched++;if(!std::equal(palmOnly.begin()+at,palmOnly.begin()+at+3,overlap.begin()+at))canopy++;}}
        check(matched>=3&&canopy>=3,"Palm canopy covered opaque airborne gull");png(evidence/"beach-bird-over-palm.png",640,360,overlap);report<<"BEACH_CONTROLLED_BIRD_OVER_PALM=PASS\n";
    }
    {
        SceneRuntime cats(gl,sceneIndex("cats"));allocate(640,360);Config cfg=config;cfg.sceneParams["cats"]["cats"]=1;cfg.sceneParams["cats"]["bushes"]=1;
        float x=640*(.06f+hash(0,sceneIndex("cats"),308)*.88f),y=360*(.4f+hash(0,sceneIndex("cats"),310)*.47f);
        auto draw=[&](){noiseTexture.bind(gl,0);cats.render(cfg,view,{},weather,30,0,stop);return capture(640,360);};
        draw();cats.probeCat(0,{x,y-40});auto behind=draw();cfg.sceneParams["cats"]["cats"]=0;auto tree=draw();
        cfg.sceneParams["cats"]["cats"]=1;cfg.sceneParams["cats"]["bushes"]=0;auto cat=draw();cfg.sceneParams["cats"]["cats"]=0;auto ground=draw();
        int hidden=0;for(size_t at=0;at<ground.size();at+=4){auto eq=[&](auto&a,auto&b){return std::equal(a.begin()+at,a.begin()+at+3,b.begin()+at);};if(!eq(tree,ground)&&!eq(cat,ground)&&eq(behind,tree))hidden++;}
        check(hidden>8,"Cat behind tree was painted in foreground");png(evidence/"cats-behind-tree.png",640,360,behind);
        cfg.sceneParams["cats"]["cats"]=1;cfg.sceneParams["cats"]["bushes"]=1;cats.probeCat(0,{x,y+8});auto front=draw();cfg.sceneParams["cats"]["bushes"]=0;cat=draw();int visible=0;
        for(size_t at=0;at<ground.size();at+=4){auto eq=[&](auto&a,auto&b){return std::equal(a.begin()+at,a.begin()+at+3,b.begin()+at);};if(!eq(tree,ground)&&!eq(cat,ground)&&eq(front,cat))visible++;}
        check(visible>8,"Cat in front of tree was painted behind it");png(evidence/"cats-in-front-of-tree.png",640,360,front);report<<"CATS_CONTROLLED_BOTH_SIDES_OF_TREE_DEPTH=PASS\n";
    }
    {
        // Owner geometry fixture, not an inferred arrangement of their screens.
        std::vector<Monitor> monitors{{L"portrait",{-1080,-500,0,1420},144,false},{L"wide",{0,0,2560,1080},96,true}};
        auto geometry=sceneGeometry(monitors,monitors[1],Layout::Panorama);
        std::vector<RECT> islands{{1190,650,1264,736},{1190,760,1264,846},{1190,870,1264,956},{1740,1100,1814,1186},{1800,1100,1874,1186},{2770,850,2844,936},{2800,960,2874,1046},{90,1300,164,1386}};
        for(const char*id:{"meadow","beach","beach-turquoise"}){
            SceneRuntime scene(gl,sceneIndex(id));scene.setGeometry(geometry);Config settings=config;settings.sceneParams[id]["palms"]=8;
            auto all=bounds(monitors);View world{0,0,float(all.right-all.left),float(all.bottom-all.top),float(all.right-all.left),float(all.bottom-all.top)};
            // Full world includes intentionally unrendered physical gaps. Only
            // individual actual output captures are evidence of visible scene.
            for(auto&m:monitors){auto v=viewport(m,all,Layout::Panorama);allocate(int(v.w),int(v.h));noiseTexture.bind(gl,0);scene.render(settings,v,islands,weather,4,0,stop);png(evidence/(std::string(id)+(m.primary?"-panorama-wide.png":"-panorama-portrait.png")),int(v.w),int(v.h),capture(int(v.w),int(v.h)));}
            if(std::string(id)!="meadow"){
                std::set<int> planted;auto layout=scene.palms();for(auto&p:layout.trees){auto b=p.transform().bounds();check(geometry.covers(b)&&iconClear(b,islands),"Palm cropped/in gap/icon");check(scene.shore(p.foot)<-6,"Palm trunk in water");check(abs(b.w/b.h-1302.f/1208)<.001f,"Palm artwork aspect changed");planted.insert(p.component);}
                check(int(planted.size())>=std::min(layout.requested,layout.feasible),"Feasible island starved before repeat planting");check(!layout.trees.empty(),"Multi-island fixture planted no palms");
                auto revision=layout.revision;allocate(640,360);noiseTexture.bind(gl,0);scene.render(settings,{0,0,640,360,world.worldW,world.worldH},islands,weather,4.1f,0,stop);check(scene.palms().revision==revision,"Idle frame recomputed placement");
                report<<id<<"_PALMS="<<layout.trees.size()<<"/"<<layout.requested<<" ISLANDS="<<planted.size()<<"/"<<layout.feasible<<" COMPONENTS="<<layout.components<<"\n";
            }
            std::ofstream diagnostic(evidence/(std::string(id)+"-bounds.txt"));for(auto b:scene.diagnosticBounds())diagnostic<<b.x<<','<<b.y<<','<<b.w<<','<<b.h<<'\n';
        }
        // Reverse primary order and use independent/sync mapping as well.
        std::reverse(monitors.begin(),monitors.end());for(auto mode:{Layout::Independent,Layout::Synchronized,Layout::Panorama})for(auto&m:monitors){auto v=viewport(m,bounds(monitors),mode);auto g=sceneGeometry(monitors,m,mode);check(g.contains({v.x+v.w*.5f,v.y+v.h*.5f}),"Reversed topology mapping missed display");}
        report<<"STAGGERED_NEGATIVE_MIXED_DPI_BOTH_ORDERS_AND_LAYOUTS=PASS\nPALM_GEOMETRY_ASPECT_ICON_COMPONENT_ALLOCATION_STABILITY=PASS\n";
    }
    {
        // Real renderer temporal sequences. Each column is a specific time,
        // including immediately after input (dt=0); no image retouching.
        std::ofstream trace(evidence/"motion-trace.txt");
        const std::array<float,9> times{0,.0f,.2f,.6f,1.2f,2.4f,4.8f,8.f,12.f};
        for(const char*id:{"ocean","beach-turquoise","aquarium","meadow"}){
            SceneRuntime scene(gl,sceneIndex(id));View v{0,0,960,540,960,540};allocate(480,270);Config cfg=config;std::vector<unsigned char> sequence(size_t(480*3)*270*3*4,255);
            std::map<std::string,std::vector<Body>> before;float elapsed=0;
            for(size_t frame=0;frame<times.size();frame++){
                if(frame==1){before=scene.actors();V2 target{670,330};if(std::string(id)=="meadow"&&scene.meadowBoy().initialized)target=scene.meadowBoy().position+V2{-90,-90};scene.click(target.x,target.y);}
                while(elapsed+.0001f<times[frame]){float step=std::min(1.f/25,times[frame]-elapsed);elapsed+=step;noiseTexture.bind(gl,0);scene.render(cfg,v,{},weather,20+elapsed,step,stop,.5f,.5f);}
                noiseTexture.bind(gl,0);scene.render(cfg,v,{},weather,20+elapsed,0,stop,.5f,.5f);auto pixels=capture(480,270);
                if(frame==1)for(auto&[name,list]:before)for(size_t j=0;j<list.size();j++)check((list[j].position-scene.actors().at(name)[j].position).length()==0,"dt=0 rendered input changed position");
                png(evidence/(std::string(id)+"-motion-"+std::to_string(frame)+".png"),480,270,pixels);
                for(int row=0;row<270;row++)memcpy(sequence.data()+(size_t((2-frame/3)*270+row)*1440+(frame%3)*480)*4,pixels.data()+size_t(row)*480*4,480*4);
                for(auto&[name,list]:scene.actors())for(size_t j=0;j<list.size();j++)if(list[j].initialized){auto&b=list[j];check(scene.spatial().contains(b.position),"Actor outside physical viewports");if((name=="uKay"||name=="uTurt"||name=="uFish")&&std::string(id)=="beach-turquoise")check(scene.shore(b.position)>12,"Swimmer/craft on land");trace<<id<<','<<frame<<','<<elapsed<<','<<name<<','<<j<<','<<b.position.x<<','<<b.position.y<<','<<b.velocity.x<<','<<b.velocity.y<<','<<b.heading<<'\n';}
                if(std::string(id)=="meadow"){auto&b=scene.meadowBoy();trace<<id<<','<<frame<<','<<elapsed<<",boy,"<<b.position.x<<','<<b.position.y<<','<<b.speed()<<'\n';}
            }
            png(evidence/(std::string(id)+"-motion-sequence.png"),1440,810,sequence);
        }
        report<<"TEMPORAL_SEQUENCE_TIMES=before,immediate,0.2,0.6,1.2,2.4,4.8,8,12 seconds\nACTUAL_RENDERER_MOTION_SEQUENCES=PASS\n";
    }
    {
        Config night=config;night.sceneParams["sky"]["time"]=3;
        SceneRuntime quiet(gl,sceneIndex("sky")),meteor(gl,sceneIndex("sky"));allocate(640,360);
        for(auto*scene:{&quiet,&meteor}){noiseTexture.bind(gl,0);scene->render(night,view,icons,weather,10.,.04f,stop);}
        meteor.click(320,120);noiseTexture.bind(gl,0);quiet.render(night,view,icons,weather,10.12,.12f,stop);auto without=capture(640,360);
        noiseTexture.bind(gl,0);meteor.render(night,view,icons,weather,10.12,.12f,stop);check(capture(640,360)!=without,"Night meteor input missing");report<<"SKY_NIGHT_INPUT=PASS\n";
    }
    {
        SceneRuntime scene(gl,sceneIndex("beach"));allocate(640,360);noiseTexture.bind(gl,0);scene.render(config,view,icons,weather,4.,0,stop);auto full=capture(640,360);
        for(int tile=0;tile<2;tile++){allocate(320,360);noiseTexture.bind(gl,0);View v{float(tile*320),0,320,360,640,360};scene.render(config,v,icons,weather,4.,0,stop);auto bytes=capture(320,360);for(int y=0;y<360;y++)for(int x=0;x<320;x++)for(int c=0;c<4;c++)check(abs(int(bytes[(y*320+x)*4+c])-int(full[(y*640+x+tile*320)*4+c]))<=1,"Panorama tile differs from same scene pixel");}
        report<<"TILED_PANORAMA_PIXEL_PARITY=PASS\n";
    }
    {
        SceneRuntime scene(gl,sceneIndex("beach"));allocate(320,180);noiseTexture.bind(gl,0);scene.render(config,view,icons,weather,4.,0,stop,.5f,.5f);capture(320,180);report<<"SCALED_SURFACE_COORDINATES=PASS\n";
    }
    for(auto shape:std::array<std::pair<int,int>,2>{{{2560,1080},{1080,1920}}}){
        int w=shape.first,h=shape.second;allocate(w,h);int id=sceneIndex(w>h?"beach":"cat-hill");SceneRuntime scene(gl,id);noiseTexture.bind(gl,0);scene.render(config,{0,0,float(w),float(h),float(w),float(h)},icons,weather,6.,.04f,stop);png(evidence/(w>h?"ultrawide-2560x1080.png":"portrait-1080x1920.png"),w,h,capture(w,h));
    }report<<"ULTRAWIDE_PORTRAIT_RENDER=PASS\n";
    // Context and scene resources are destroyed on their owner thread even after
    // a shader failure; verify a following scene still compiles and renders.
    try{Program broken(gl,"#version 330 core\nthis is intentionally invalid");throw std::runtime_error("Bad shader accepted");}catch(const std::exception&){}
    allocate(640,360);{SceneRuntime recovery(gl,sceneIndex("sky"));noiseTexture.bind(gl,0);recovery.render(config,view,icons,weather,8.,.04f,stop);capture(640,360);}report<<"SHADER_FAILURE_RECOVERY=PASS\n";
    gl.BindFramebuffer(0x8D40,0);gl.DeleteFramebuffers(1,&fbo);gl.DeleteVertexArrays(1,&vao);
    auto await=[&](auto condition,const char*label){uint64_t end=GetTickCount64()+30000;while(!condition()){check(GetTickCount64()<end,label);Sleep(20);}};
    {
        Snapshot s;s.config.enabled=true;s.config.scene=sceneIndex("beach");s.config.fps=15;s.monitors={{L"CI-hosted",{0,0,640,360},96,true}};Session session(s);
        await([&]{auto m=session.metrics();if(!m.error.empty())throw std::runtime_error(m.error);return m.frames>=4;},"Wallpaper session never presented");
        s.locked=true;session.publish(s);await([&]{return session.metrics().paused;},"Lock did not suspend");auto frames=session.metrics().frames;Sleep(150);check(session.metrics().frames==frames,"Rendering continued during lock");
        s.locked=false;s.displayOff=true;session.publish(s);Sleep(150);check(session.metrics().frames==frames,"Rendering continued with display off");
        s.displayOff=false;s.config.scene=sceneIndex("lake");session.publish(s);await([&]{return session.metrics().frames>frames+2;},"Resume/scene swap failed");
        s.monitors[0].rect={-320,-100,320,260};session.publish(s);frames=session.metrics().frames;await([&]{return session.metrics().frames>frames+2;},"Hotplug/reposition failed");
        s.opaqueWindows={s.monitors[0].rect};session.publish(s);await([&]{return session.metrics().paused;},"Fullscreen/occlusion did not pause");frames=session.metrics().frames;check(session.metrics().scenes==1,"Covered assigned scene state discarded");Sleep(150);check(session.metrics().frames==frames,"Rendering continued while fully covered");
        s.opaqueWindows.clear();session.publish(s);await([&]{return session.metrics().frames>frames+2;},"Uncover did not resume");
        auto active=session.metrics();report<<"SESSION_FRAMES="<<active.frames<<"\nSESSION_MISSED_DEADLINES="<<active.missed<<"\nSESSION_LAST_RENDER_MS="<<active.lastFrameMs<<"\nSESSION_OCCLUSION_STATE_RETENTION=PASS\n";
        session.stop();await([&]{return session.finished();},"Wallpaper shutdown failed");auto m=session.metrics();check(m.surfaces==0&&m.scenes==0&&!m.running,"Disabled wallpaper resources remain");
        report<<"SESSION_NATIVE_PRESENT="<<m.presentation<<"\nSESSION_LOCK_DISPLAY_OFF_RESUME=PASS\nSESSION_SCENE_SWAP_TOPOLOGY=PASS\nSESSION_STOP_RESOURCE_RELEASE=PASS\n";
    }
    context.current();report<<"OWNER_PERFORMANCE=NOT_MEASURED\nOWNER_VISUAL_UAT=PENDING\n";return 0;
}catch(const std::exception&e){std::cerr<<"WALLPAPER_RENDERER_FAILURE="<<e.what()<<'\n';return 1;}}
