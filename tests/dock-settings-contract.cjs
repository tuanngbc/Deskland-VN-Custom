'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const html=fs.readFileSync(require('node:path').join(__dirname,'../prototype/settings.html'),'utf8');
const start=html.indexOf('let dockOptions='),end=html.indexOf("for(const id of ['systemMonitorEnabled'",start);
assert(start>0&&end>start,'integrated settings handlers must exist');
const controls=new Map(),messages=[];
const $=id=>{if(!controls.has(id))controls.set(id,{checked:false,value:'',textContent:'',listeners:{},classList:{toggle(){}},addEventListener(name,handler){this.listeners[name]=handler;}});return controls.get(id);};
const context=vm.createContext({$,send:m=>messages.push(m),t:key=>key==='dockRunning'?'Translated running':key,esc:value=>String(value).replace(/&/g,'&amp;').replace(/"/g,'&quot;')});
vm.runInContext(html.slice(start,end),context);
let count=0;function test(name,fn){fn();count++;console.log('PASS '+name);}
test('saved settings and tray updates use the same controls',()=>{
  context.syncDockSettings({dock:{enabled:false,stageEnabled:true,allWindows:true,stageSide:'right'},wallpaperEnabled:false,dockRunning:true});
  assert.equal($('dockEnabled').checked,false);assert.equal($('dockStageEnabled').checked,true);
  assert.equal($('dockAllWindows').checked,true);assert.equal($('dockStageSide').value,'right');
  assert.equal($('dockWallpaperEnabled').checked,false);assert.equal($('dockRuntimeStatus').textContent,'Translated running');
});
test('editing one module preserves the other module and scope',()=>{
  $('dockEnabled').checked=true;$('dockEnabled').listeners.change();
  const message=messages.pop();assert.equal(message.t,'dock');assert.equal(message.enabled,true);
  assert.equal(Object.keys(message).sort().join(','),'enabled,t');
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
  context.syncDockSettings({dockError:null,dockRunning:true});assert.equal($('dockRuntimeStatus').textContent,'Translated running');
});
test('independent display modes update only their own targets and preview',()=>{
  const monitors=[{id:'landscape',name:'Main',w:2560,h:1080,dpi:96,primary:true},{id:'portrait',name:'Vertical',w:1080,h:1920,dpi:120,primary:false}];
  context.syncDockSettings({dockDisplays:monitors,dock:{enabled:true,stageEnabled:true,allWindows:true,
    dockDisplay:{mode:'primary'},stageDisplay:{mode:'selected',selected:'portrait'}}});
  assert.equal($('dockDisplayMode').value,'primary');assert.equal($('stageDisplayMode').value,'selected');
  assert.match($('stageDisplayChoices').innerHTML,/value="portrait" selected/);
  assert.match($('dockDisplayPreview').innerHTML,/<small>Dock<\/small>.+<small>Stage Manager<\/small>/);
  $('stageDisplayMode').listeners.change({target:{value:'custom'}});
  const m=messages.pop();
  assert.equal(Object.keys(m).sort().join(','),'stageDisplay,t');
  assert.equal(m.stageDisplay.mode,'custom');
  $('stageDisplayChoices').listeners.change({target:{hasAttribute:key=>key==='data-display-id',checked:true,dataset:{displayId:'portrait'}}});
  assert.equal(JSON.stringify(messages.pop().stageDisplay.custom),JSON.stringify(['landscape','portrait']));
  assert.equal($('dockDisplayMode').value,'primary');
});
test('disconnected saved selection falls back and is restored without writing settings',()=>{
  const main={id:'landscape',name:'Main',w:2560,h:1080,dpi:96,primary:true};
  context.syncDockSettings({dockDisplays:[main],dock:{stageEnabled:true,stageDisplay:{mode:'selected',selected:'portrait'}}});
  assert.equal($('dockDisplayFallback').hidden,false);
  assert.match($('stageDisplayChoices').innerHTML,/disabled selected/);
  assert.match($('dockDisplayPreview').innerHTML,/<small>Dock · Stage Manager<\/small>/);
  context.syncDockSettings({dockDisplays:[main,{...main,id:'portrait',name:'Vertical',primary:false}]});
  assert.equal($('dockDisplayFallback').hidden,true);
  assert.match($('stageDisplayChoices').innerHTML,/value="portrait" selected/);
  assert.equal(messages.length,0);
});
test('appearance edits do not overwrite placement, scope, or pins',()=>{
  context.syncDockSettings({dock:{appearance:{theme:'dark',iconSize:52,position:'left'}}});
  assert.equal($('dockLook_iconSize').value,'52');
  assert.equal($('dockLook_position').value,'left');
  $('dockLook_transparency').value='64';$('dockLook_transparency').listeners.change();
  const m=messages.pop();
  assert.equal(Object.keys(m).sort().join(','),'appearance,t');
  assert.equal(m.appearance.theme,'dark');
  assert.equal(m.appearance.iconSize,52);
  assert.equal(m.appearance.transparency,64);
  assert.equal(m.appearance.position,'left');
});
test('stage layout changes one display and leaves saved disconnected layouts untouched',()=>{
  const main={id:'landscape',name:'Main',w:2560,h:1080,dpi:96,primary:true};
  const vertical={...main,id:'portrait',name:'Vertical',primary:false};
  context.syncDockSettings({dockDisplays:[main,vertical],dock:{stageEnabled:true,stageSide:'left',
    stageDisplay:{mode:'all'},stageStyles:{portrait:{side:'right',scale:115,spacing:10},offline:{spacing:13}}}});
  assert.match($('dockDisplayPreview').innerHTML,/dock-display-stage right/);
  $('stageStyleDisplay').listeners.change({target:{value:'portrait'}});
  assert.equal($('stageStyle_side').value,'right');
  assert.equal($('stageStyle_scale').value,'115');
  $('stageStyle_spacing').value='16';$('stageStyle_spacing').listeners.change();
  assert.deepEqual(JSON.parse(JSON.stringify(messages.pop())),{t:'stageStyle',display:'portrait',style:{side:'right',scale:115,spacing:16,groupLimit:5,opacity:76}});
  $('stageStyle_groupLimit').value='8';$('stageStyle_groupLimit').listeners.change();
  assert.equal(messages.pop().style.groupLimit,8);
  $('stageStyle_opacity').value='55';$('stageStyle_opacity').listeners.change();
  assert.deepEqual(JSON.parse(JSON.stringify(messages.pop().style)),{side:'right',scale:115,spacing:16,groupLimit:8,opacity:55});
});
test('utility order, visibility and reset are isolated to the selected monitor',()=>{
  const d={id:'main',name:'Main',w:2560,h:1080,dpi:96,primary:true};
  context.syncDockSettings({dockDisplays:[d,{...d,id:'portrait',primary:false}],dock:{utilityProfiles:{main:{enabled:false}}}});
  $('utilityDisplay').listeners.change({target:{value:'portrait'}});
  assert.equal($('utilityEnabled').checked,true);
  context.moveUtility('calendar',0);
  const first=messages.pop();
  assert.equal(first.t,'utilityProfile');
  assert.equal(first.display,'portrait');
  assert.equal(first.profile.order[0],'calendar');
  $('utilityRows').listeners.change({target:{checked:false,dataset:{utilityVisible:'audio'}}});
  assert.equal(messages.pop().profile.hidden[0],'audio');
  $('utilityRows').listeners.click({target:{closest:s=>s==='[data-utility-pin]'?{dataset:{utilityPin:'calendar'}}:null}});
  assert.equal(messages.pop().profile.inline[0],'calendar');
  $('utilityReset').listeners.click();
  assert.equal(messages.pop().profile.hidden.length,0);
  $('utilityDisplay').listeners.change({target:{value:'main'}});
  assert.equal($('utilityEnabled').checked,false);
});
test('window policy edits one output without overwriting its other module',()=>{
  const main={id:'main',name:'Main',w:2560,h:1080,dpi:96,primary:true};
  const portrait={...main,id:'portrait',primary:false};
  context.syncDockSettings({dockDisplays:[main,portrait],dock:{windowPolicies:{main:{dock:'floating'},portrait:{stage:'autoHide'}}}});
  $('dockPolicyDisplay').listeners.change({target:{value:'portrait'}});
  assert.equal($('dockPolicy_stage').value,'autoHide');
  $('dockPolicy_dock').value='reserve';$('dockPolicy_dock').listeners.change();
  assert.equal(JSON.stringify(messages.pop()),JSON.stringify({t:'dockWindowPolicy',display:'portrait',policy:{dock:'reserve',stage:'autoHide'}}));
  $('dockPolicyDisplay').listeners.change({target:{value:'main'}});
  assert.equal($('dockPolicy_dock').value,'floating');
});
console.log('TOTAL: '+count+' PASS / 0 FAIL');
