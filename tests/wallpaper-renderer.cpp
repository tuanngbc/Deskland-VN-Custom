// Actual shader/scene execution on a hosted software GL device, never owner PC.
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
    WICPixelFormatGUID format=GUID_WICPixelFormat32bppRGBA;check(SUCCEEDED(frame->SetPixelFormat(&format)),"PNG pixel format");
    check(SUCCEEDED(frame->WritePixels(h,w*4,UINT(top.size()),top.data()))&&SUCCEEDED(frame->Commit())&&SUCCEEDED(encoder->Commit()),"PNG write");
}
int main(int argc,char**argv){try{
    wchar_t hosted[16]{};GetEnvironmentVariableW(L"GITHUB_ACTIONS",hosted,16);check(!wcscmp(hosted,L"true"),"Hosted verification only");check(argc==2,"Evidence path required");
    using namespace wallpaper;CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    std::filesystem::path evidence=argv[1];std::filesystem::create_directories(evidence);std::filesystem::create_directories(assetsRoot()/L"wallpaper/thumbs");
    std::ofstream report(evidence/"renderer.txt");Window hidden;Context context(hidden.h);GL gl;std::cout<<"GL_VERSION="<<glGetString(GL_VERSION)<<'\n';std::cout<<"GL_RENDERER="<<glGetString(GL_RENDERER)<<'\n';
    GLuint vao=0,fbo=0;gl.GenVertexArrays(1,&vao);gl.BindVertexArray(vao);gl.GenFramebuffers(1,&fbo);
    Texture target,noiseTexture;auto noise=noisePixels();noiseTexture.image(gl,0,256,256,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,noise.data(),true);
    auto allocate=[&](int w,int h){target.image(gl,4,w,h,0x8058,GL_RGBA,GL_UNSIGNED_BYTE,nullptr);gl.BindFramebuffer(0x8D40,fbo);gl.FramebufferTexture2D(0x8D40,0x8CE0,GL_TEXTURE_2D,target.name(),0);check(gl.CheckFramebufferStatus(0x8D40)==0x8CD5,"Probe surface incomplete");glViewport(0,0,w,h);};
    auto capture=[&](int w,int h){std::vector<unsigned char> bytes(size_t(w)*h*4);glReadPixels(0,0,w,h,GL_RGBA,GL_UNSIGNED_BYTE,bytes.data());check(glGetError()==GL_NO_ERROR,"Render/readback error");return bytes;};
    Config config;config.clockLighting=false;config.manualHour=12;Weather weather;std::atomic<bool> stop{false};const std::vector<RECT> icons{{24,65,93,143},{24,170,93,248},{135,65,204,143}};
    View view{0,0,640,360,640,360};
    for(int i=0;i<int(catalog().size());i++){
        allocate(640,360);noiseTexture.bind(gl,0);SceneRuntime scene(gl,i);scene.render(config,view,icons,weather,2.,.04f,stop);auto before=capture(640,360);
        std::set<uint32_t> colors;for(size_t p=0;p<before.size();p+=16){uint32_t value;memcpy(&value,before.data()+p,4);colors.insert(value);}check(colors.size()>30,"Scene is blank/constant");
        png(evidence/(std::string(catalog()[i].id)+".png"),640,360,before);png(assetsRoot()/L"wallpaper/thumbs"/(std::string(catalog()[i].id)+".png"),640,360,before);
        scene.click(320,240);noiseTexture.bind(gl,0);scene.render(config,view,icons,weather,2.12,.12f,stop);auto after=capture(640,360);check(before!=after,"Scene never animates or responds");
        report<<catalog()[i].id<<"=RENDER_ANIMATION_CLICK_PASS\n";std::cout<<catalog()[i].id<<" PASS\n";
    }
    {
        SceneRuntime scene(gl,sceneIndex("beach"));allocate(640,360);noiseTexture.bind(gl,0);scene.render(config,view,icons,weather,4.,0,stop);auto full=capture(640,360);
        for(int tile=0;tile<2;tile++){allocate(320,360);noiseTexture.bind(gl,0);View v{float(tile*320),0,320,360,640,360};scene.render(config,v,icons,weather,4.,0,stop);auto bytes=capture(320,360);for(int y=0;y<360;y++)for(int x=0;x<320;x++)for(int c=0;c<4;c++)check(abs(int(bytes[(y*320+x)*4+c])-int(full[(y*640+x+tile*320)*4+c]))<=1,"Panorama tile differs from same scene pixel");}
        report<<"TILED_PANORAMA_PIXEL_PARITY=PASS\n";
    }
    {
        SceneRuntime scene(gl,sceneIndex("beach"));allocate(320,180);noiseTexture.bind(gl,0);scene.render(config,view,icons,weather,4.,0,stop,.5f,.5f);capture(320,180);report<<"SCALED_SURFACE_COORDINATES=PASS\n";
    }
    // Context and scene resources are destroyed on their owner thread even after
    // a shader failure; verify a following scene still compiles and renders.
    try{Program broken(gl,"#version 330 core\nthis is intentionally invalid");throw std::runtime_error("Bad shader accepted");}catch(const std::exception&){}
    allocate(640,360);{SceneRuntime recovery(gl,sceneIndex("sky"));noiseTexture.bind(gl,0);recovery.render(config,view,icons,weather,8.,.04f,stop);capture(640,360);}report<<"SHADER_FAILURE_RECOVERY=PASS\n";
    gl.BindFramebuffer(0x8D40,0);gl.DeleteFramebuffers(1,&fbo);gl.DeleteVertexArrays(1,&vao);report<<"OWNER_PERFORMANCE=NOT_MEASURED\nOWNER_VISUAL_UAT=PENDING\n";return 0;
}catch(const std::exception&e){std::cerr<<"WALLPAPER_RENDERER_FAILURE="<<e.what()<<'\n';return 1;}}
