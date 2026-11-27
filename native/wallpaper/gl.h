// Owner-thread-only OpenGL objects. No legacy GL loader or renderer is linked.
#pragma once
#include <windows.h>
#include <GL/gl.h>
#include <string>
#include <stdexcept>
#include <vector>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <map>

namespace wallpaper {
using GLsizeiptr=ptrdiff_t;
class GL {
    HMODULE library=nullptr;
    template<class T>T load(const char*name){auto p=wglGetProcAddress(name);auto i=reinterpret_cast<intptr_t>(p);
        if(!p||i==1||i==2||i==3||i==-1)p=reinterpret_cast<PROC>(GetProcAddress(library,name));
        if(!p)throw std::runtime_error(std::string("OpenGL 3.3 unavailable: ")+name);return reinterpret_cast<T>(p);}
public:
#define WP_GL(ret,name,args) using name##Fn=ret(APIENTRY*)args;name##Fn name=nullptr;
    WP_GL(GLuint,CreateShader,(GLenum)) WP_GL(void,ShaderSource,(GLuint,GLsizei,const char*const*,const GLint*))
    WP_GL(void,CompileShader,(GLuint)) WP_GL(void,GetShaderiv,(GLuint,GLenum,GLint*))
    WP_GL(void,GetShaderInfoLog,(GLuint,GLsizei,GLsizei*,char*)) WP_GL(void,DeleteShader,(GLuint))
    WP_GL(GLuint,CreateProgram,()) WP_GL(void,AttachShader,(GLuint,GLuint)) WP_GL(void,LinkProgram,(GLuint))
    WP_GL(void,GetProgramiv,(GLuint,GLenum,GLint*)) WP_GL(void,GetProgramInfoLog,(GLuint,GLsizei,GLsizei*,char*))
    WP_GL(void,DeleteProgram,(GLuint)) WP_GL(void,UseProgram,(GLuint)) WP_GL(GLint,GetUniformLocation,(GLuint,const char*))
    WP_GL(void,Uniform1i,(GLint,GLint)) WP_GL(void,Uniform1f,(GLint,GLfloat)) WP_GL(void,Uniform2f,(GLint,GLfloat,GLfloat))
    WP_GL(void,Uniform4fv,(GLint,GLsizei,const GLfloat*)) WP_GL(void,ActiveTexture,(GLenum))
    WP_GL(void,GenVertexArrays,(GLsizei,GLuint*)) WP_GL(void,BindVertexArray,(GLuint)) WP_GL(void,DeleteVertexArrays,(GLsizei,const GLuint*))
    WP_GL(void,GenFramebuffers,(GLsizei,GLuint*)) WP_GL(void,BindFramebuffer,(GLenum,GLuint))
    WP_GL(void,FramebufferTexture2D,(GLenum,GLenum,GLenum,GLuint,GLint)) WP_GL(GLenum,CheckFramebufferStatus,(GLenum))
    WP_GL(void,DeleteFramebuffers,(GLsizei,const GLuint*))
    WP_GL(void,GenRenderbuffers,(GLsizei,GLuint*)) WP_GL(void,DeleteRenderbuffers,(GLsizei,const GLuint*))
    WP_GL(void,FramebufferRenderbuffer,(GLenum,GLenum,GLenum,GLuint))
    WP_GL(void,GenBuffers,(GLsizei,GLuint*)) WP_GL(void,DeleteBuffers,(GLsizei,const GLuint*))
    WP_GL(void,BindBuffer,(GLenum,GLuint)) WP_GL(void,BufferData,(GLenum,GLsizeiptr,const void*,GLenum))
    WP_GL(void,GetBufferSubData,(GLenum,ptrdiff_t,GLsizeiptr,void*))
    WP_GL(void*,FenceSync,(GLenum,GLbitfield)) WP_GL(GLenum,ClientWaitSync,(void*,GLbitfield,uint64_t)) WP_GL(void,DeleteSync,(void*))
#undef WP_GL
    GL(){library=LoadLibraryExW(L"opengl32.dll",nullptr,LOAD_LIBRARY_SEARCH_APPLICATION_DIR|LOAD_LIBRARY_SEARCH_SYSTEM32);if(!library)throw std::runtime_error("OpenGL loader unavailable");
        try {
#define WP_LOAD(name) name=load<name##Fn>("gl" #name);
        WP_LOAD(CreateShader) WP_LOAD(ShaderSource) WP_LOAD(CompileShader) WP_LOAD(GetShaderiv) WP_LOAD(GetShaderInfoLog) WP_LOAD(DeleteShader)
        WP_LOAD(CreateProgram) WP_LOAD(AttachShader) WP_LOAD(LinkProgram) WP_LOAD(GetProgramiv) WP_LOAD(GetProgramInfoLog) WP_LOAD(DeleteProgram)
        WP_LOAD(UseProgram) WP_LOAD(GetUniformLocation) WP_LOAD(Uniform1i) WP_LOAD(Uniform1f) WP_LOAD(Uniform2f) WP_LOAD(Uniform4fv) WP_LOAD(ActiveTexture)
        WP_LOAD(GenVertexArrays) WP_LOAD(BindVertexArray) WP_LOAD(DeleteVertexArrays) WP_LOAD(GenFramebuffers) WP_LOAD(BindFramebuffer)
        WP_LOAD(FramebufferTexture2D) WP_LOAD(CheckFramebufferStatus) WP_LOAD(DeleteFramebuffers)
        WP_LOAD(GenRenderbuffers) WP_LOAD(DeleteRenderbuffers) WP_LOAD(FramebufferRenderbuffer)
        WP_LOAD(GenBuffers) WP_LOAD(DeleteBuffers) WP_LOAD(BindBuffer) WP_LOAD(BufferData) WP_LOAD(GetBufferSubData)
        WP_LOAD(FenceSync) WP_LOAD(ClientWaitSync) WP_LOAD(DeleteSync)
#undef WP_LOAD
        }catch(...){FreeLibrary(library);library=nullptr;throw;}
    }
    ~GL(){if(library)FreeLibrary(library);}GL(const GL&)=delete;GL&operator=(const GL&)=delete;
};
inline std::filesystem::path assetsRoot(){wchar_t p[32768];DWORD n=GetModuleFileNameW(nullptr,p,32768);if(!n||n>=32768)throw std::runtime_error("Executable path unavailable");return std::filesystem::path(p).parent_path();}
inline std::string assetText(const std::filesystem::path&p){
    std::ifstream f(p,std::ios::binary);if(!f)throw std::runtime_error("Missing wallpaper shader");
    f.seekg(0,std::ios::end);auto n=f.tellg();if(n<0||n>2*1024*1024)throw std::runtime_error("Shader exceeds budget");
    std::string s(size_t(n),'\0');f.seekg(0);if(n&&!f.read(s.data(),n))throw std::runtime_error("Shader read failed");return s;
}
inline std::string desktopShader(std::string text,bool world=true){
    auto line=text.find('\n');if(line==std::string::npos)throw std::runtime_error("Missing GLSL version");
    text.replace(0,line,"#version 330 core");line=text.find('\n');
    if(world)text.insert(line+1,"uniform vec2 wpOrigin,wpPixelScale;\nvec4 wpFrag(){return vec4(gl_FragCoord.xy/wpPixelScale+wpOrigin,gl_FragCoord.zw);}\n#define gl_FragCoord wpFrag()\n");
    for(const auto& token:std::vector<std::string>{"precision highp float;","precision mediump float;","precision lowp float;"}){size_t at;while((at=text.find(token))!=std::string::npos)text.erase(at,token.size());}
    size_t at;while((at=text.find("uniform highp "))!=std::string::npos)text.replace(at,14,"uniform ");return text;
}
class Program {
    GL*gl=nullptr;GLuint id=0;std::map<std::string,GLint> locations;
    GLuint shader(GLenum type,const std::string&s){GLuint sh=gl->CreateShader(type);if(!sh)throw std::runtime_error("Shader allocation failed");const char*p=s.c_str();gl->ShaderSource(sh,1,&p,nullptr);gl->CompileShader(sh);GLint ok=0;gl->GetShaderiv(sh,0x8B81,&ok);
        if(!ok){std::string log(8192,'\0');gl->GetShaderInfoLog(sh,8191,nullptr,log.data());gl->DeleteShader(sh);throw std::runtime_error(log.c_str());}return sh;}
public:
    Program(GL&g,const std::string&fragment):gl(&g){
        const std::string vert="#version 330 core\nvoid main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));gl_Position=vec4(p*2.-1.,0.,1.);}";
        GLuint vs=0,fs=0;try{vs=shader(0x8B31,vert);fs=shader(0x8B30,fragment);id=g.CreateProgram();if(!id)throw std::runtime_error("Program allocation failed");g.AttachShader(id,vs);g.AttachShader(id,fs);g.LinkProgram(id);GLint ok=0;g.GetProgramiv(id,0x8B82,&ok);if(!ok){std::string log(8192,'\0');g.GetProgramInfoLog(id,8191,nullptr,log.data());throw std::runtime_error(log.c_str());}g.DeleteShader(vs);g.DeleteShader(fs);}
        catch(...){if(vs)g.DeleteShader(vs);if(fs)g.DeleteShader(fs);if(id)g.DeleteProgram(id);id=0;throw;}
    }
    ~Program(){if(id)gl->DeleteProgram(id);}Program(const Program&)=delete;
    void use(){gl->UseProgram(id);}GLint loc(const char*n){auto i=locations.find(n);if(i!=locations.end())return i->second;GLint l=gl->GetUniformLocation(id,n);locations.emplace(n,l);return l;}
    void f(const char*n,float v){gl->Uniform1f(loc(n),v);}void i(const char*n,int v){gl->Uniform1i(loc(n),v);}void v2(const char*n,float x,float y){gl->Uniform2f(loc(n),x,y);}
    void v4(const char*n,const float*p,int count=1){gl->Uniform4fv(loc(n),count,p);}void v4(const char*n,std::initializer_list<float> p){if(p.size()%4==0)gl->Uniform4fv(loc(n),GLsizei(p.size()/4),p.begin());}
};
class Texture {
    GLuint id=0;
public:
    Texture(){glGenTextures(1,&id);if(!id)throw std::runtime_error("Texture allocation failed");}
    ~Texture(){if(id)glDeleteTextures(1,&id);}Texture(const Texture&)=delete;
    GLuint name()const{return id;}void bind(GL&g,int slot){g.ActiveTexture(0x84C0+slot);glBindTexture(GL_TEXTURE_2D,id);}
    void image(GL&g,int slot,int width,int height,GLint internal,GLenum format,GLenum type,const void*data,bool repeat=false){
        if(width<=0||height<=0||uint64_t(width)*height>16*1024*1024)throw std::runtime_error("Texture size exceeds budget");
        bind(g,slot);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_LINEAR);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,GL_LINEAR);
        glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_S,repeat?GL_REPEAT:0x812F);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_T,repeat?GL_REPEAT:0x812F);
        glTexImage2D(GL_TEXTURE_2D,0,internal,width,height,0,format,type,data);if(glGetError()!=GL_NO_ERROR)throw std::runtime_error("Texture allocation/upload failed");
    }
};
class Context {
    HWND window=nullptr;HDC dc=nullptr;HGLRC rc=nullptr;
public:
    explicit Context(HWND h):window(h){
        dc=GetDC(h);PIXELFORMATDESCRIPTOR p{};p.nSize=sizeof(p);p.nVersion=1;p.dwFlags=PFD_DRAW_TO_WINDOW|PFD_SUPPORT_OPENGL|PFD_DOUBLEBUFFER;p.iPixelType=PFD_TYPE_RGBA;p.cColorBits=32;p.cAlphaBits=8;
        try{int index=ChoosePixelFormat(dc,&p);if(!index||!SetPixelFormat(dc,index,&p))throw std::runtime_error("Wallpaper pixel format unavailable");
            rc=wglCreateContext(dc);if(!rc||!wglMakeCurrent(dc,rc))throw std::runtime_error("Wallpaper context unavailable");
            using Create=HGLRC(WINAPI*)(HDC,HGLRC,const int*);auto create=reinterpret_cast<Create>(wglGetProcAddress("wglCreateContextAttribsARB"));
            if(!create)throw std::runtime_error("OpenGL 3.3 is required");const int attributes[]={0x2091,3,0x2092,3,0x9126,1,0};
            HGLRC core=create(dc,nullptr,attributes);if(!core)throw std::runtime_error("OpenGL 3.3 core unavailable");wglMakeCurrent(nullptr,nullptr);wglDeleteContext(rc);rc=core;
            if(!wglMakeCurrent(dc,rc))throw std::runtime_error("Wallpaper context activation failed");
            using Interval=BOOL(WINAPI*)(int);auto interval=reinterpret_cast<Interval>(wglGetProcAddress("wglSwapIntervalEXT"));if(interval)interval(0);
        }catch(...){if(rc){wglMakeCurrent(nullptr,nullptr);wglDeleteContext(rc);rc=nullptr;}if(dc){ReleaseDC(window,dc);dc=nullptr;}throw;}
    }
    ~Context(){if(rc){wglMakeCurrent(nullptr,nullptr);wglDeleteContext(rc);}if(dc)ReleaseDC(window,dc);}Context(const Context&)=delete;
    bool current(){return wglMakeCurrent(dc,rc)!=FALSE;}bool current(HDC target){return wglMakeCurrent(target,rc)!=FALSE;}
    bool present(){return SwapBuffers(dc)!=FALSE;}
};
}
