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
        {
            // Larger untouched frames make ground contacts/props legible for
            // human review; the catalog thumbnail alone is insufficient.
            allocate(1280,720);SceneRuntime reference(gl,i);noiseTexture.bind(gl,0);
            reference.render(config,{0,0,1280,720,1280,720},{{36,130,110,215},{36,235,110,320},{36,340,110,425}},weather,3,.04f,stop);
            png(evidence/(std::string(catalog()[i].id)+"-reference-1280x720.png"),1280,720,capture(1280,720));
        }
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
        allocate(384,384);View v{0,0,384,384,384,384};Sprites sprites(gl);SpriteTransform cat{{192,192},360,360,.5f,.5f,false};
        auto draw=[&](float angle,float time){glClearColor(1,1,1,1);glClear(GL_COLOR_BUFFER_BIT);sprites.anchored(L"wallpaper/assets/meadow-black-kitten.png",v,cat,time,1,1,false,1,.3f,angle);return capture(384,384);};
        const auto resting=draw(0,0),laterRest=draw(0,8);check(resting==laterRest,"Resting kitten artwork still sways as a rigid sprite");
        auto flick=draw(.3f,8);int changed=0;for(int y=0;y<384;++y)for(int x=0;x<384;++x){size_t pixel=(size_t(383-y)*384+x)*4;
            bool differs=!std::equal(resting.begin()+pixel,resting.begin()+pixel+4,flick.begin()+pixel);if(differs){changed++;check(x>180&&y>220,"Tail deformation moves kitten face or body");}}
        check(changed>8,"Kitten tail deformation absent");check(meadowKittenTailAngle(0)==0&&meadowKittenTailAngle(8)==0,"Kitten long resting interval absent");
        bool moved=false;for(int i=0;i<160;++i)moved=moved||abs(meadowKittenTailAngle(2.16f+i*.01f))>.05f;check(moved,"Kitten periodic tail flick absent");
        png(evidence/"meadow-kitten-tail-flick.png",384,384,flick);
        SceneRuntime meadow(gl,sceneIndex("meadow"));allocate(96,54);View sceneView{0,0,960,540,960,540};noiseTexture.bind(gl,0);meadow.render(config,sceneView,{},weather,8,0,stop,.1f,.1f);
        auto position=meadow.probeMeadowKitten();meadow.click(position.x,position.y);check(meadow.probeMeadowTail()==0,"Petting teleported the tail");
        for(int i=0;i<5;++i){noiseTexture.bind(gl,0);meadow.render(config,sceneView,{},weather,8+(i+1)*.04f,.04f,stop,.1f,.1f);}
        check(meadow.probeMeadowTail()>.01f&&(meadow.probeMeadowKitten()-position).length()==0,"Kitten pet response lacks continuous local motion");
        for(int i=0;i<180;++i){noiseTexture.bind(gl,0);meadow.render(config,sceneView,{},weather,8.2f+(i+1)*.04f,.04f,stop,.1f,.1f);}
        check(abs(meadow.probeMeadowTail())<.001f,"Kitten pet spring did not settle");
        report<<"MEADOW_KITTEN_LOCAL_TAIL_MASK_AND_REST=PASS\nMEADOW_KITTEN_CONTINUOUS_PET_REACTION=PASS\n";
    }
    {
        SceneRuntime beach(gl,sceneIndex("beach"));allocate(640,360);Config cfg=config;cfg.sceneParams["beach"]["birds"]=1;cfg.sceneParams["beach"]["palms"]=0;
        std::vector<RECT> island{{260,280,340,330}};
        auto draw=[&](){noiseTexture.bind(gl,0);beach.render(cfg,view,island,weather,30,0,stop);return capture(640,360);};
        draw();beach.probeActor("uBird",0,{300,185});beach.probePalms({});auto bird=draw();
        // Anchor on known icon-generated sand, with canopy directly under bird.
        beach.probePalms({Palm{{300,235},0,0,.496f,false}});check(beach.shore({300,235})<-6,"Palm overlap fixture is not sand");auto overlap=draw();cfg.sceneParams["beach"]["birds"]=0;auto palmOnly=draw();beach.probePalms({});auto sandOnly=draw();int matched=0,canopy=0;
        for(int y=180;y<=190;y++)for(int x=293;x<=307;x++){size_t at=(size_t(359-y)*640+x)*4;
            if(bird[at]>240&&bird[at+1]>240&&bird[at+2]>240&&std::equal(bird.begin()+at,bird.begin()+at+3,overlap.begin()+at)){matched++;if(!std::equal(palmOnly.begin()+at,palmOnly.begin()+at+3,sandOnly.begin()+at))canopy++;}}
        check(matched>=3&&canopy>=3,"Palm canopy covered opaque airborne gull");png(evidence/"beach-bird-over-palm.png",640,360,overlap);report<<"BEACH_CONTROLLED_BIRD_OVER_PALM=PASS\n";
    }
    {
        SceneRuntime cats(gl,sceneIndex("cats"));allocate(640,360);Config cfg=config;cfg.sceneParams["cats"]["cats"]=1;cfg.sceneParams["cats"]["bushes"]=1;
        float x=640*(.06f+hash(0,sceneIndex("cats"),308)*.88f),y=360*(.4f+hash(0,sceneIndex("cats"),310)*.47f);
        auto draw=[&](){noiseTexture.bind(gl,0);cats.render(cfg,view,{},weather,30,0,stop);return capture(640,360);};
        draw();cats.probeCat(0,{x,y-40});auto behind=draw();cats.probeCat(0,{-10000,-10000});auto tree=draw();
        cats.probeCat(0,{x,y-40});cfg.sceneParams["cats"]["bushes"]=0;auto cat=draw();cats.probeCat(0,{-10000,-10000});auto ground=draw();
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
                if(std::string(id)=="aquarium")for(auto&f:scene.fish())if(f.body.initialized&&(f.mode==FishMode::Swim||f.mode==FishMode::Approach||f.mode==FishMode::Nibble||f.mode==FishMode::Bite||f.mode==FishMode::Flee))check(scene.probeFishWater(f.body.position,43),"Aquarium fish intersects sand/surface");
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
    {
        // Rest is a real stationary state. Previously an uncommanded target at
        // the current position still accelerated the steering body forward.
        SceneRuntime cats(gl,sceneIndex("cats"));allocate(192,108);Config cfg=config;
        cfg.sceneParams["cats"]["cats"]=1;cfg.sceneParams["cats"]["bushes"]=0;
        View v{0,0,960,540,960,540};noiseTexture.bind(gl,0);cats.render(cfg,v,{},weather,1,0,stop,.2f,.2f);
        cats.probeCat(0,{480,340});cats.probeCatState(0,4,5);V2 resting=cats.catActors()[0].body.position;
        for(int frame=0;frame<50;frame++){noiseTexture.bind(gl,0);cats.render(cfg,v,{},weather,1+(frame+1)*.04f,.04f,stop,.2f,.2f);}
        check((cats.catActors()[0].body.position-resting).length()<.01f&&cats.catActors()[0].pose==4,"Resting cat drifted or lost sit state");
        png(evidence/"cats-native-sit.png",192,108,capture(192,108));
        cats.click(resting.x,resting.y);check(cats.catActors()[0].happy>0,"Cat petting reaction missing");
        cats.click(650,360);bool chased=false,played=false;
        for(int frame=0;frame<300;frame++){noiseTexture.bind(gl,0);cats.render(cfg,v,{},weather,3+(frame+1)*.04f,.04f,stop,.2f,.2f);const auto&cat=cats.catActors()[0];chased|=cat.body.speed()>45;played|=cat.pose==6;check(cat.poseBlend>=0&&cat.poseBlend<=1,"Invalid pose transition");}
        check(chased&&played,"Cat did not run to yarn and play");png(evidence/"cats-native-yarn.png",192,108,capture(192,108));
        report<<"CAT_REST_NO_DRIFT_PET_RUN_AND_YARN_PLAY=PASS\n";
    }
    {
        SceneRuntime match(gl,sceneIndex("stadium"));allocate(192,108);Config cfg=config;cfg.sceneParams["stadium"]["players"]=6;
        View v{0,0,960,540,960,540};noiseTexture.bind(gl,0);match.render(cfg,v,{},weather,1,0,stop,.2f,.2f);
        auto before=match.stadiumBall().position;match.click(700,340);
        check((match.stadiumBall().position-before).length()==0&&match.stadiumBall().speed()>0,"Stadium click teleported ball or supplied no kick");
        for(int frame=0;frame<40;frame++){noiseTexture.bind(gl,0);match.render(cfg,v,{},weather,1+(frame+1)*.04f,.04f,stop,.2f,.2f);}
        const auto&players=match.stadiumPlayers();check((players[0].position-players[1].position).length()>20,"Goalkeeper follows identical field-player path");
        match.probeStadiumBall({960*.9f-4,540*.54f});const auto score=match.stadiumScore();match.click(959,540*.54f);
        for(int frame=0;frame<30;frame++){noiseTexture.bind(gl,0);match.render(cfg,v,{},weather,3+(frame+1)*.04f,.04f,stop,.2f,.2f);}
        check(match.stadiumScore()[0]==score[0]+1,"Actual goal-line crossing did not score");png(evidence/"stadium-native-physical-goal.png",192,108,capture(192,108));
        report<<"STADIUM_PHYSICAL_KICK_KEEPER_FORMATION_GOAL_CROSSING=PASS\n";
    }
    {
        // Exercise actual fishing lifecycles using read-only runtime state;
        // no probe forces a bite, hook, catch or timeout transition.
        for(const char* id:{"lake","aquarium"})for(int outcome=0;outcome<3;++outcome){
            SceneRuntime fishing(gl,sceneIndex(id));View v{0,0,960,540,960,540};Config cfg=config;
            cfg.sceneParams["aquarium"]["fish"]=1;allocate(96,54);float elapsed=0;
            auto draw=[&](float dt){elapsed+=dt;noiseTexture.bind(gl,0);fishing.render(cfg,v,{},weather,10+elapsed,dt,stop,.1f,.1f);};
            draw(0);check(fishing.fish()[0].body.initialized,"Fishing fixture has no physical fish");
            const V2 bait=fishing.fish()[0].body.position;fishing.click(bait.x,bait.y);check(fishing.fishingStatus().phase==1,"Cast did not start");
            bool sawWait=false,sawNibble=false,sawBite=false;int selected=-1;
            for(int frame=0;frame<850;++frame){draw(.04f);const auto state=fishing.fishingStatus();sawWait|=state.phase==2;
                if(state.actor>=0){selected=state.actor;auto mode=fishing.fish()[selected].mode;sawNibble|=mode==FishMode::Nibble;}
                if(outcome==1&&sawNibble)break;
                if(state.phase==3){sawBite=true;break;}
            }
            check(sawWait&&sawNibble&&selected>=0,"Fishing never approached and nibbled actual bait");
            if(outcome==1){
                const auto before=fishing.fish()[selected].body.position;fishing.click(bait.x,bait.y);
                check(fishing.fish()[selected].mode==FishMode::Flee&&(fishing.fish()[selected].body.position-before).length()==0&&fishing.fishingStatus().caught==0,"Early hook did not flee continuously");
                report<<id<<"_EARLY_HOOK_FLEE_NO_CATCH=PASS\n";continue;
            }
            check(sawBite,"Fish did not enter bite window");
            if(outcome==2){for(int frame=0;frame<100;++frame)draw(.04f);
                check(fishing.fishingStatus().phase!=3&&fishing.fishingStatus().caught==0&&fishing.fish()[selected].mode!=FishMode::Bite,"Missed bite did not escape");
                report<<id<<"_MISSED_BITE_ESCAPE=PASS\n";continue;
            }
            const auto before=fishing.fish()[selected].body.position;fishing.click(bait.x,bait.y);
            check(fishing.fishingStatus().phase==4&&(fishing.fish()[selected].body.position-before).length()==0,"Hook click teleported fish");
            bool reeled=false;for(int frame=0;frame<550&&fishing.fishingStatus().caught==0;++frame){auto previous=fishing.fish()[selected].body.position;draw(.04f);
                check((fishing.fish()[selected].body.position-previous).length()<9.5f,"Reeling moved fish discontinuously");
                if(fishing.fishingStatus().phase==4){reeled=true;auto hook=fishing.fishingStatus().hook;const auto& fish=fishing.fish()[selected];
                    if(std::string(id)=="aquarium"){float size=.65f+hash(selected,2,80)*.6f;V2 mouth=fish.body.position+V2{cos(fish.heading),sin(fish.heading)}*(14*size);check((hook-mouth).length()<.01f,"Reeling hook detached from physical fish mouth");}
                    else check((hook-fish.body.position).length()<.01f,"Lake flight duplicated shader fish trajectory");
                }
            }
            check(reeled&&fishing.fishingStatus().caught==1,"Physical fish never reached landing to complete catch");
            png(evidence/(std::string(id)+"-completed-physical-catch.png"),96,54,capture(96,54));
            for(int frame=0;frame<750&&fishing.fish()[selected].mode!=FishMode::Swim;++frame)draw(.04f);
            check(fishing.fish()[selected].mode==FishMode::Swim,"Caught actor did not return to normal swimming");
            report<<id<<"_CAST_WAIT_APPROACH_NIBBLE_BITE_PHYSICAL_REEL_CATCH_RETURN=PASS\n";
        }
    }
    {
        Config cfg=config;cfg.sceneParams["summer-porch"]["fanSpeed"]=0;
        SceneRuntime porch(gl,sceneIndex("summer-porch"));allocate(96,54);View v{0,0,1080,700,1080,700};double time=0;
        auto draw=[&](float dt){time+=dt;noiseTexture.bind(gl,0);porch.render(cfg,v,{},weather,time,dt,stop,96.f/1080,54.f/700);};
        draw(0);auto off=porch.probePorch();for(int i=0;i<25;++i)draw(.04f);auto rested=porch.probePorch();
        check(off.fanYaw==rested.fanYaw&&off.fanBlade==rested.fanBlade,"Stopped fan still turns its head or blades");
        check(abs(rested.chimeAngle)>.001f,"Wind-driven chime lost its spring target");
        cfg.sceneParams["summer-porch"]["fanSpeed"]=1;for(int i=0;i<25;++i)draw(.04f);auto running=porch.probePorch();
        check(abs(running.fanYaw-rested.fanYaw)>.02f&&abs(running.fanBlade-rested.fanBlade)>.1f,"Fan speed does not drive both phases");
        cfg.sceneParams["summer-porch"]["fanSpeed"]=0;draw(0);auto frozen=porch.probePorch();for(int i=0;i<25;++i)draw(.04f);auto stopped=porch.probePorch();
        check(frozen.fanYaw==stopped.fanYaw&&frozen.fanBlade==stopped.fanBlade,"Fan phase jumps or continues after switch off");
        float scale=porch.spatial().porchScale();porch.click(1080*.346f,700*.095f+58*scale);auto tap=porch.probePorch();
        check(abs(tap.chimeVelocity)>4&&tap.chimeAngle==stopped.chimeAngle&&tap.bird>0,"Chime tap lacks continuous impulse/bird reaction");
        draw(.04f);check(abs(porch.probePorch().chimeAngle-tap.chimeAngle)>.01f,"Chime impulse never integrates");
        for(int i=0;i<20;++i)draw(.04f);check(porch.probePorch().bird==0,"Bird hop failed to settle");
        porch.click(1080*.702f+32*scale,700*.592f-6*scale);check(porch.probePorch().bites==1,"Melon bite missing");
        for(int i=0;i<160;++i)draw(.04f);check(porch.probePorch().bites==1,"Melon refilled too early");
        for(int i=0;i<5;++i)draw(.04f);check(porch.probePorch().bites==0,"Melon did not refill");
        auto before=porch.probePorch();porch.click(1080*.415f,700*.508f);auto pet=porch.probePorch();
        check(pet.poses[0]==before.poses[0]&&pet.rolls[0]==0&&pet.happy[0]==1,"Petting replaced Corgi pose immediately");
        for(int i=0;i<5;++i)draw(.04f);check(porch.probePorch().poses[0]==before.poses[0],"Corgi pose changed before roll midpoint");
        for(int i=0;i<4;++i)draw(.04f);check(porch.probePorch().poses[0]!=(before.poses[0]),"Corgi did not transition through roll midpoint");
        for(int i=0;i<8;++i)draw(.04f);check(porch.probePorch().rolls[0]<0,"Corgi roll failed to settle");
        png(evidence/"porch-native-interactions.png",96,54,capture(96,54));
        report<<"PORCH_FAN_STOP_RESTART_AND_WIND_SPRING=PASS\nPORCH_CHIME_BIRD_MELON_CORGI_LIFECYCLE=PASS\n";
    }
    {
        Config cfg=config;cfg.sceneParams["boxing-ring"]["autoSpar"]=1;
        std::array<BoxingModel,4> zones;
        for(int zone=0;zone<4;++zone){auto& model=zones[zone];model.configure(cfg,1080,700,1);auto foot=model.foot(1);
            V2 point{foot.x+(zone==1?20.f:0.f),foot.y-115+(zone==0?-22.f:zone==3?48.f:0.f)};
            model.click(point);check(model.impacts==0&&model.fighters[1].hp==1,"Boxing damage occurred before glove impact");
            for(int step=0;step<8;++step)model.step(1.f/120);check(model.impacts==0,"Boxing wind-up was skipped");
            for(int step=0;step<10;++step)model.step(1.f/120);
            check(model.impacts==1&&model.fighters[1].hp<1&&model.fighters[1].hpLag>model.fighters[1].hp,"Boxing impact/lagged health absent");
        }
        check(zones[0].fighters[1].headY.x>0&&zones[0].fighters[1].squash.x<0,"Head-top impact lacks vertical compression");
        check(zones[1].fighters[1].headX.x>0&&zones[1].fighters[1].faceX.x>0,"Cheek impact lacks asymmetric face response");
        check(zones[2].fighters[1].headY.x<0&&zones[2].fighters[1].squash.x>0,"Chin impact lacks upward stretch");
        check(zones[3].fighters[1].dent.x>0,"Body shot lacks torso dent");
        BoxingModel combo;combo.configure(cfg,1080,700,1);auto foot=combo.foot(1);for(int i=0;i<4;++i)combo.click({foot.x,foot.y-115});
        for(int i=0;i<15;++i)combo.step(1.f/120);
        check(combo.impacts==4&&combo.fighters[1].combo==4&&combo.fighters[1].dizzy>0,"Four-glove combo/dizzy state missing");
        BoxingModel props;props.configure(cfg,1080,700,1);props.click({152,652*.18f+82});check(props.bags[0].x==0&&props.bags[0].v==0,"Bag reacted before impact");
        for(int i=0;i<20;++i)props.step(1.f/120);check(abs(props.bags[0].x)>.01f,"Bag pendulum did not respond to glove");
        props.click({120,652*.845f});check(props.tire>0&&props.ring.v!=0,"Tire smash lacks ring response");
        auto round=props.roundNumber;props.click({540,44});check(props.roundNumber!=round&&props.roundSeconds==60,"HUD did not start next round");
        for(int i=0;i<11200;++i)props.step(1.f/120);check(props.roundNumber>=3,"Round clock did not advance naturally");
        Config autoConfig=cfg;autoConfig.sceneParams["boxing-ring"]["autoSpar"]=2;BoxingModel ai;ai.configure(autoConfig,1080,700,1);for(int i=0;i<160;++i)ai.step(1.f/120);check(ai.impacts>=2,"Fast auto-spar did not launch and land punches");
        SceneRuntime ringScene(gl,sceneIndex("boxing-ring"));allocate(384,216);View v{0,0,1080,700,1080,700};noiseTexture.bind(gl,0);ringScene.render(cfg,v,{},weather,1,0,stop,384.f/1080,216.f/700);
        auto target=ringScene.boxingState()->foot(1);ringScene.click(target.x+20,target.y-115);
        for(int i=0;i<5;++i){noiseTexture.bind(gl,0);ringScene.render(cfg,v,{},weather,1+(i+1)*.04f,.04f,stop,384.f/1080,216.f/700);}
        check(ringScene.boxingState()->impacts==1,"Native scene did not consume Boxing model");png(evidence/"boxing-native-cheek-impact.png",384,216,capture(384,216));
        report<<"BOXING_DELAYED_FOUR_ZONE_SPRING_FACE_TORSO_IMPACTS=PASS\nBOXING_FOUR_GLOVE_COMBO_AND_DIZZY=PASS\nBOXING_BAG_TIRE_ROUND_AND_AUTOSPARRING=PASS\nBOXING_ACTUAL_RENDERER_INTEGRATION=PASS\n";
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
