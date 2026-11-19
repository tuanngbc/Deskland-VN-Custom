'use strict';
// Run on GitHub Actions only. Captures the exact native mesh, shader sources and
// simulation frames exported by the Rust binary; never installs the application.
const fs=require('node:fs'),path=require('node:path'),{pathToFileURL}=require('node:url');
const assert=require('node:assert/strict'),crypto=require('node:crypto');
const {chromium}=require('playwright');
const [input,output]=process.argv.slice(2);
assert(input&&output,'Usage: render-whale-preview data.json output-directory');
fs.mkdirSync(output,{recursive:true});
const data=JSON.parse(fs.readFileSync(input,'utf8'));
const html=String.raw`<!doctype html><html lang="vi"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Cá voi · Bản xem trước</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#e7f0ee;color:#244244;font:14px 'Segoe UI',sans-serif}
main{max-width:1280px;margin:auto}header{height:58px;display:flex;align-items:center;justify-content:space-between;padding:0 22px}
h1{font-size:18px;font-weight:600;margin:0}header span{font-size:12px;color:#647a79}
canvas{width:100%;display:block;aspect-ratio:8/5;background:#14a4ba}
footer{display:flex;align-items:center;gap:14px;padding:13px 20px;background:#f8fbfa;min-height:65px}
button{border:1px solid #cbdad7;background:white;border-radius:8px;padding:8px 13px;color:#244244;cursor:pointer;font:inherit}
button[aria-pressed=true]{background:#e0eeea;border-color:#99b4aa}
input{flex:1;min-width:60px;accent-color:#3c7e79}#stage{min-width:155px;font-size:12px;color:#58736f}#time{font-variant-numeric:tabular-nums}
</style><main><header><h1>Cá voi · Xem chuyển động mới</h1><span>Chưa đưa vào nền đang dùng</span></header>
<canvas id="scene" width="1280" height="800"></canvas>
<footer><button id="play">Tạm dừng</button><button id="detail" aria-pressed="true">Cận cảnh</button><button id="wide" aria-pressed="false">Toàn cảnh</button><input id="seek" type="range" min="0" max="36" step="0.02"><span id="time">0:00</span><span id="stage"></span></footer></main>
<script>
const D=__DATA__;
const canvas=document.getElementById('scene');
const gl=canvas.getContext('webgl2',{antialias:true,alpha:false,preserveDrawingBuffer:true});
if(!gl)throw Error('WebGL2 unavailable');
gl.getExtension('OES_texture_float_linear');
const uniforms=new Map();
function source(text){return text.replace('#version 330 core','#version 300 es\nprecision highp float;\nprecision highp int;');}
function shader(type,text){const s=gl.createShader(type);gl.shaderSource(s,source(text));gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s));return s;}
function program(v,f){const p=gl.createProgram();const vs=shader(gl.VERTEX_SHADER,v),fs=shader(gl.FRAGMENT_SHADER,f);gl.attachShader(p,vs);gl.attachShader(p,fs);gl.linkProgram(p);gl.deleteShader(vs);gl.deleteShader(fs);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p));return p;}
function uniform(p,name,value,integer=false){let cache=uniforms.get(p);if(!cache)uniforms.set(p,cache=new Map());if(!cache.has(name))cache.set(name,gl.getUniformLocation(p,name));const u=cache.get(name);if(u===null)return;if(integer)gl.uniform1i(u,value);else if(Array.isArray(value)){if(value.length===2)gl.uniform2fv(u,value);else gl.uniform4fv(u,value);}else gl.uniform1f(u,value);}
const empty=gl.createVertexArray();gl.bindVertexArray(empty);
const fullscreen='#version 300 es\nprecision highp float;void main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));gl_Position=vec4(p*2.0-1.0,0.0,1.0);}';
const beach=program(fullscreen,D.beachFragment);
const skin=program(D.skinVertex,D.skinFragment),water=program(D.waterVertex,D.waterFragment);
function texture(unit,w,h,internal,format,type,values,repeat=false){const t=gl.createTexture();gl.activeTexture(gl.TEXTURE0+unit);gl.bindTexture(gl.TEXTURE_2D,t);gl.texImage2D(gl.TEXTURE_2D,0,internal,w,h,0,format,type,values);for(const key of [gl.TEXTURE_MIN_FILTER,gl.TEXTURE_MAG_FILTER])gl.texParameteri(gl.TEXTURE_2D,key,gl.LINEAR);for(const key of [gl.TEXTURE_WRAP_S,gl.TEXTURE_WRAP_T])gl.texParameteri(gl.TEXTURE_2D,key,repeat?gl.REPEAT:gl.CLAMP_TO_EDGE);return t;}
const noise=texture(0,D.noiseSize,D.noiseSize,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array(D.noise),true);
const field=texture(1,D.fieldWidth,D.fieldHeight,gl.R16F,gl.RED,gl.FLOAT,new Float32Array(D.field));
const backdrop=texture(2,D.width,D.height,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,null);
const fbo=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fbo);gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,backdrop,0);
if(gl.checkFramebufferStatus(gl.FRAMEBUFFER)!==gl.FRAMEBUFFER_COMPLETE)throw Error('Incomplete backdrop framebuffer');
gl.viewport(0,0,D.width,D.height);gl.useProgram(beach);
for(const [name,value] of Object.entries({uRes:[D.width,D.height],uGrid:[D.fieldWidth,D.fieldHeight],uScale:1,uSeed:(71%997)*0.37,uStep:5,uMix:1,uTime:6,uSpeed:22,uRunup:120,uShallow:260,uShoal:110,uGrain:1,uDebug:0,uWave:1.6,uSigma:85,uSpacing:70,uWaves:1,uKayN:0,uBirdN:0,uShadowOffset:[0,0],uShadowAlpha:0}))uniform(beach,name,value);
uniform(beach,'uNoise',0,true);uniform(beach,'uFA',1,true);uniform(beach,'uFB',1,true);gl.drawArrays(gl.TRIANGLES,0,3);
gl.bindFramebuffer(gl.FRAMEBUFFER,null);
const backdropProgram=program(fullscreen,'#version 300 es\nprecision highp float;uniform sampler2D uImage;out vec4 o;void main(){o=texture(uImage,gl_FragCoord.xy/vec2(1280.0,800.0));}');
const mesh=gl.createVertexArray();gl.bindVertexArray(mesh);const buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(D.mesh),gl.STATIC_DRAW);
for(const [index,size,offset] of [[0,3,0],[1,3,12],[2,2,24]]){gl.enableVertexAttribArray(index);gl.vertexAttribPointer(index,size,gl.FLOAT,false,32,offset);}
const count=D.mesh.length/8;
let detail=true,playing=false,start=0,position=0;
function rowAt(t,rows){const f=Math.max(0,Math.min(t*D.fps,rows.length-1));const i=Math.floor(f),j=Math.min(i+1,rows.length-1),u=f-i;return rows[i].map((v,k)=>v+(rows[j][k]-v)*u);}
function frameAt(t){const f=Math.max(0,Math.min(t*D.fps,D.frames.length-1));const i=Math.floor(f),j=Math.min(i+1,D.frames.length-1),u=f-i;return D.frames[i].map((row,n)=>row.map((v,k)=>v+(D.frames[j][n][k]-v)*u));}
function actor(row){const whale=row.slice(0,4),pose=row.slice(4,8),stroke=row.slice(8,12),effect=row.slice(12,16);
 gl.bindVertexArray(empty);gl.useProgram(water);uniform(water,'uRes',[D.width,D.height]);uniform(water,'uWhale',whale);uniform(water,'uPose',pose);uniform(water,'uEffect',effect);gl.drawArrays(gl.TRIANGLE_STRIP,0,4);
 gl.bindVertexArray(mesh);gl.useProgram(skin);uniform(skin,'uRes',[D.width,D.height]);uniform(skin,'uWhale',whale);uniform(skin,'uPose',pose);uniform(skin,'uStroke',stroke);gl.enable(gl.CULL_FACE);gl.frontFace(gl.CW);gl.drawArrays(gl.TRIANGLES,0,count);gl.frontFace(gl.CCW);gl.disable(gl.CULL_FACE);
}
window.renderAt=function(t){position=Math.min(36,Math.max(0,t));gl.bindVertexArray(empty);gl.disable(gl.BLEND);gl.useProgram(backdropProgram);uniform(backdropProgram,'uImage',2,true);gl.drawArrays(gl.TRIANGLES,0,3);gl.enable(gl.BLEND);gl.blendFunc(gl.ONE,gl.ONE_MINUS_SRC_ALPHA);const rows=detail?[rowAt(position,D.detail)]:frameAt(position);for(const row of rows)actor(row);gl.disable(gl.BLEND);const error=gl.getError();if(error!==gl.NO_ERROR)throw Error('WebGL error '+error);document.getElementById('seek').value=position;document.getElementById('time').textContent='0:'+Math.floor(position).toString().padStart(2,'0');document.getElementById('stage').textContent=detail?(position<4?'Nổi lên':position<8?'Nổi và phun nước':position<12?'Lướt và lấy đà':position<16?'Cong lưng, chúi xuống':position<20?'Nâng đuôi, lặn sâu':position<29?'Bơi dưới mặt nước':'Trở lên mặt nước'):'Ba con bơi lệch nhịp';return rows;};
window.setView=function(view){detail=view==='detail';document.getElementById('detail').setAttribute('aria-pressed',detail);document.getElementById('wide').setAttribute('aria-pressed',!detail);window.renderAt(position);};
window.pausePreview=function(){playing=false;document.getElementById('play').textContent='Phát';};
window.playPreview=function(){start=performance.now()-position*1000;playing=true;document.getElementById('play').textContent='Tạm dừng';};
document.getElementById('play').onclick=()=>playing?window.pausePreview():window.playPreview();
document.getElementById('detail').onclick=()=>window.setView('detail');document.getElementById('wide').onclick=()=>window.setView('wide');document.getElementById('seek').oninput=e=>{window.pausePreview();window.renderAt(+e.target.value);};
function tick(now){if(playing)window.renderAt(((now-start)/1000)%36);requestAnimationFrame(tick);}requestAnimationFrame(tick);window.renderAt(6);window.previewReady={vertices:count,actors:D.frames[0].length,renderer:gl.getParameter(gl.RENDERER)};window.playPreview();
</script></html>`;
const htmlPath=path.join(output,'whale-preview.html');
fs.writeFileSync(htmlPath,html.replace('__DATA__',JSON.stringify(data).replaceAll('<','\\u003c')));
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 try{
  const errors=[];
  const context=await browser.newContext({viewport:{width:1280,height:923},deviceScaleFactor:1});
  const page=await context.newPage();page.on('pageerror',e=>errors.push(String(e)));
  await page.goto(pathToFileURL(htmlPath).href);await page.waitForFunction(()=>window.previewReady);
  await page.evaluate(()=>window.pausePreview());
  const readiness=await page.evaluate(()=>window.previewReady);assert.equal(readiness.actors,3);assert(readiness.vertices<7000);
  for(const [label,time] of [['surface',6],['arch',12.8],['flukes',16.5],['entry',19.4],['deep',25]]){
   await page.evaluate(t=>window.renderAt(t),time);await page.screenshot({path:path.join(output,'whale-'+label+'.png')});
  }
  await page.evaluate(()=>{window.setView('wide');window.renderAt(5);});await page.screenshot({path:path.join(output,'whale-scene.png')});
  await context.close();
  const recording=await browser.newContext({viewport:{width:1024,height:768},deviceScaleFactor:1,recordVideo:{dir:output,size:{width:1024,height:768}}});
  const videoPage=await recording.newPage();videoPage.on('pageerror',e=>errors.push(String(e)));
  await videoPage.goto(pathToFileURL(htmlPath).href);await videoPage.waitForFunction(()=>window.previewReady);
  await videoPage.evaluate(()=>{window.pausePreview();window.setView('detail');window.renderAt(0);window.playPreview();});
  await videoPage.waitForTimeout(36_500);await recording.close();
  const recorded=await videoPage.video().path();fs.renameSync(recorded,path.join(output,'whale-motion.webm'));
  assert.deepEqual(errors,[]);assert(fs.statSync(path.join(output,'whale-motion.webm')).size>20_000);
  const manifest={commit:process.env.GITHUB_SHA||null,...readiness,meshSha256:crypto.createHash('sha256').update(JSON.stringify(data.mesh)).digest('hex'),images:6,seconds:36,errors};
  fs.writeFileSync(path.join(output,'preview-manifest.json'),JSON.stringify(manifest,null,2));
  console.log('PASS native mesh/shaders exported; six screenshots and full motion cycle captured',JSON.stringify(manifest));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
