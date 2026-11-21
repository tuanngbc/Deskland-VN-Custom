'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const html=fs.readFileSync(require('node:path').join(__dirname,'../prototype/settings.html'),'utf8');
const start=html.indexOf('let dockOptions='),end=html.indexOf("for(const id of ['systemMonitorEnabled'",start);
assert(start>0&&end>start,'integrated settings handlers must exist');
const controls=new Map(),messages=[];
const $=id=>{if(!controls.has(id))controls.set(id,{checked:false,value:'',textContent:'',listeners:{},classList:{toggle(){}},addEventListener(name,handler){this.listeners[name]=handler;}});return controls.get(id);};
const context=vm.createContext({$,send:m=>messages.push(m),t:key=>key});
vm.runInContext(html.slice(start,end),context);
let count=0;function test(name,fn){fn();count++;console.log('PASS '+name);}
test('saved settings and tray updates use the same controls',()=>{
  context.syncDockSettings({dock:{enabled:false,stageEnabled:true,allWindows:true,stageSide:'right'},wallpaperEnabled:false,dockRunning:true});
  assert.equal($('dockEnabled').checked,false);assert.equal($('dockStageEnabled').checked,true);
  assert.equal($('dockAllWindows').checked,true);assert.equal($('dockStageSide').value,'right');
  assert.equal($('dockWallpaperEnabled').checked,false);assert.equal($('dockRuntimeStatus').textContent,'dockRunning');
});
test('editing one module preserves the other module and scope',()=>{
  $('dockEnabled').checked=true;$('dockEnabled').listeners.change();
  const message=messages.pop();assert.equal(message.t,'dock');assert.equal(message.enabled,true);
  assert.equal(message.stageEnabled,true);assert.equal(message.allWindows,true);assert.equal(message.stageSide,'right');
  assert(!('pinned' in message),'UI must not overwrite the canonical pin list');
});
test('wallpaper toggle is its own message and does not reset dock settings',()=>{
  $('dockWallpaperEnabled').checked=true;$('dockWallpaperEnabled').listeners.change();
  assert.equal(JSON.stringify(messages.pop()),JSON.stringify({t:'wallpaperEnabled',enabled:true}));
  assert.equal($('dockStageEnabled').checked,true);
});
test('startup failures are visible and a subsequent host update clears them',()=>{
  context.syncDockSettings({dockError:'Native window creation failed',dockRunning:false});
  assert.equal($('dockRuntimeStatus').textContent,'Native window creation failed');
  context.syncDockSettings({dockError:null,dockRunning:true});assert.equal($('dockRuntimeStatus').textContent,'dockRunning');
});
console.log('TOTAL: '+count+' PASS / 0 FAIL');
