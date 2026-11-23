'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const html=fs.readFileSync(require('node:path').join(__dirname,'../prototype/settings.html'),'utf8');
const start=html.indexOf('let dockOptions='),end=html.indexOf("for(const id of ['systemMonitorEnabled'",start);
assert(start>0&&end>start,'integrated settings handlers must exist');
const controls=new Map(),messages=[];
class Option {constructor(text,value){this.text=text;this.value=value;this.dataset={};}}

const $=id=>{if(!controls.has(id))controls.set(id,{checked:false,value:'',textContent:'',listeners:{},options:[],add(option){option.remove=()=>{this.options=this.options.filter(x=>x!==option);};this.options.push(option);},replaceChildren(...options){this.options=options;this.value=options[0]?.value||'';},checkValidity(){return true;},classList:{toggle(){}},addEventListener(name,handler){this.listeners[name]=handler;}});return controls.get(id);};
const context=vm.createContext({$,Option,send:m=>messages.push(m),t:key=>key==='dockRunning'?'Translated running':key,esc:value=>String(value).replace(/&/g,'&amp;').replace(/"/g,'&quot;')});
vm.runInContext(html.slice(start,end),context);
let count=0;function test(name,fn){fn();count++;console.log('PASS '+name);}
test('saved settings and tray updates use the same controls',()=>{
  context.syncDockSettings({dock:{enabled:false,stageEnabled:true,allWindows:true,stageSide:'right'},wallpaperEnabled:false,dockRunning:true});
  assert.equal($('dockEnabled').checked,false);assert.equal($('dockStageEnabled').checked,true);
  assert.equal($('dockAllWindows').checked,true);assert.equal($('dockStageSide').value,'right');
  assert.equal($('dockWallpaperEnabled').checked,false);assert.equal($('dockRuntimeStatus').textContent,'Translated running');
});
test('command pin sends the full executable and argument string intact',()=>{
  const command=String.raw`"E:\Panda Studio\venv\Scripts\pythonw.exe" "E:\Panda Studio\main.py" --title "A & B"`;
  $('dockPinCommand').value=command;
  $('dockPinLabel').value='PandaTV';
  $('dockPinSubmit').onclick();
  assert.deepEqual(JSON.parse(JSON.stringify(messages.pop())),{t:'dockPinCommand',command,label:'PandaTV'});
});
test('native shell buttons are opt-in and do not issue a taskbar hide command',()=>{
  context.syncDockSettings({dock:{nativeShellButtons:false}});
  assert.equal($('dockShellButtons').value,'local');
  $('dockShellButtons').onchange({target:{value:'windows'}});
  assert.deepEqual(JSON.parse(JSON.stringify(messages.pop())),{t:'dock',nativeShellButtons:true});
});
test('a command correction waits for the user to apply it',()=>{
  $('dockPinCommand').focus=()=>{};
  $('dockPinCommand').value='original';
  context.setDockPinResult({message:'Path was not found',suggestion:'verified command'});
  assert.equal($('dockPinCommand').value,'original');
  assert.equal($('dockPinCorrection').hidden,false);
  $('dockPinCorrection').onclick();
  assert.equal($('dockPinCommand').value,'verified command');
  assert.equal($('dockPinCorrection').hidden,true);
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
test('whole Dock scale and Start icon preserve the other appearance settings',()=>{
  context.syncDockSettings({dock:{appearance:{size:110,iconSize:52,position:'left'}}});
  const icon=$('dockLook_startIcon');
  assert.equal(icon.value,'windows');
  icon.value='orb';icon.listeners.change();
  const updated=messages.pop();
  assert.equal(updated.appearance.startIcon,'orb');
  assert.equal(updated.appearance.iconSize,52);
  assert.equal(updated.appearance.size,110);
  assert.equal(updated.appearance.position,'left');
  context.syncDockSettings({dock:{appearance:updated.appearance}});
  assert.equal(icon.value,'orb');
  $('dockLook_size').value='125';
  $('dockLook_size').listeners.change();
  const scaled=messages.pop();
  assert.equal(scaled.appearance.size,125);
  assert.equal(scaled.appearance.iconSize,52);
  assert.equal(scaled.appearance.startIcon,'orb');
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
  assert.deepEqual(JSON.parse(JSON.stringify(messages.pop())),{t:'stageStyle',display:'portrait',style:{cardHeight:0,theme:'auto',side:'right',scale:115,spacing:16,groupLimit:5,opacity:76,groupPresentation:'stacked'}});
  $('stageStyle_groupLimit').value='8';$('stageStyle_groupLimit').listeners.change();
  assert.equal(messages.pop().style.groupLimit,8);
  $('stageStyle_opacity').value='55';$('stageStyle_opacity').listeners.change();
  assert.deepEqual(JSON.parse(JSON.stringify(messages.pop().style)),{cardHeight:0,theme:'auto',side:'right',scale:115,spacing:16,groupLimit:8,opacity:55,groupPresentation:'stacked'});
  $('stageStyle_groupPresentation').value='flat';$('stageStyle_groupPresentation').listeners.change();
  assert.equal(messages.pop().style.groupPresentation,'flat');
});
test('stage theme changes only its display and leaves Dock colors alone',()=>{
  const d={id:'main',name:'Main',w:1920,h:1080,dpi:96,primary:true};
  context.syncDockSettings({dockDisplays:[d,{...d,id:'portrait',primary:false}],
    dock:{appearance:{theme:'dark'},stageStyles:{main:{theme:'light'},portrait:{theme:'dark'}}}});
  $('stageStyleDisplay').listeners.change({target:{value:'main'}});
  assert.equal($('dockLook_theme').value,'dark');
  assert.equal($('stageStyle_theme').value,'light');
  $('stageStyle_theme').value='auto';
  $('stageStyle_theme').listeners.change();
  const msg=messages.pop();
  assert.equal(msg.t,'stageStyle');assert.equal(msg.display,'main');assert.equal(msg.style.theme,'auto');
  $('stageStyleDisplay').listeners.change({target:{value:'portrait'}});
  assert.equal($('stageStyle_theme').value,'dark');assert.equal($('dockLook_theme').value,'dark');
});
test('stage presentation and each height scope send distinct canonical edits',()=>{
  const group='exe:c:/apps/paint.exe';
  context.syncDockSettings({dockDisplays:[{id:'main',name:'Main',w:1920,h:1080,dpi:96,primary:true}],
    dock:{stagePresentation:'dwm',stageCardHeight:120,stageCardOverrides:{[group]:101},stageStyles:{main:{cardHeight:144}}}});
  assert.equal($('stagePreviewMode').value,'dwm');
  assert.equal($('stageDefaultHeight').value,'120');
  assert.equal($('stageStyle_cardHeight').value,'144');
  assert.equal($('stageOverrideHeight').value,101);
  $('stagePreviewMode').listeners.change({target:{value:'perspective'}});
  assert.equal(messages.pop().mode,'perspective');
  $('stageDefaultHeight').listeners.change({target:{value:'126'}});
  assert.equal(messages.pop().height,126);
  $('stageStyle_cardHeight').value='90';$('stageStyle_cardHeight').listeners.change();
  assert.equal(messages.pop().style.cardHeight,90);
  $('stageOverrideReset').listeners.click();
  assert.deepEqual(JSON.parse(JSON.stringify(messages.pop())),{t:'stageGroupHeight',group,height:0});
});
test('utility order, visibility and reset are isolated to the selected monitor',()=>{
  const d={id:'main',name:'Main',w:2560,h:1080,dpi:96,primary:true};
  context.syncDockSettings({dockDisplays:[d,{...d,id:'portrait',primary:false}],dock:{utilityProfiles:{main:{enabled:false}}}});
  $('utilityDisplay').listeners.change({target:{value:'portrait'}});
  assert.equal($('utilityNetworkOnDock').checked,true);
  assert.equal($('utilitySoundOnDock').checked,true);
  context.moveUtility('bluetooth',0);
  const first=messages.pop();
  assert.equal(first.t,'utilityProfile');
  assert.equal(first.display,'portrait');
  assert.equal(first.profile.order[0],'bluetooth');
  $('utilityRows').listeners.change({target:{checked:false,dataset:{utilityVisible:'audio'}}});
  assert.equal(messages.pop().profile.hidden[0],'audio');
  $('utilityReset').listeners.click();
  assert.equal(messages.pop().profile.hidden.length,0);
  $('utilityDisplay').listeners.change({target:{value:'main'}});
  assert.equal($('utilityNetworkOnDock').checked,false);
  assert.equal($('utilitySoundOnDock').checked,false);
});
test('Dock network, sound and metrics visibility are independent',()=>{
  const d={id:'signals-test',name:'Main',w:2560,h:1080,dpi:96,primary:true};
  context.syncDockSettings({dockDisplays:[d],dock:{utilityProfiles:{[d.id]:{enabled:false,
    networkOnDock:false,soundOnDock:true,metrics:{enabled:true,mode:'inline',mask:31,interval:2,compact:true}}}}});
  assert.equal($('utilityNetworkOnDock').checked,false);
  assert.equal($('utilitySoundOnDock').checked,true);
  assert.equal($('utilityMetricMode').checked,true);
  $('utilitySoundOnDock').checked=false;
  $('utilitySoundOnDock').listeners.change();
  assert.equal(messages.pop().profile.metrics.mode,'inline');
  $('utilityNetworkOnDock').checked=true;
  $('utilityNetworkOnDock').listeners.change();
  assert.equal(messages.pop().profile.soundOnDock,false);
  $('utilityMetricMode').checked=false;
  $('utilityMetricMode').listeners.change();
  const profile=messages.pop().profile;
  assert.equal(profile.metrics.mode,'popover');
  assert.equal(profile.networkOnDock,true);
  assert.equal(profile.soundOnDock,false);
});
test('taskbar selector preserves user intent while separately reporting the verified state',()=>{
  context.syncDockSettings({dock:{enabled:true},dockTaskbarHidden:false});
  const control=$('dockShellMode');
  assert.equal(control.value,'windows');
  assert.equal(control.disabled,false);
  control.listeners.change({target:{value:"dock"}});
  assert.deepEqual(JSON.parse(JSON.stringify(messages.pop())),{t:'dockTaskbar',hidden:true});
  context.syncDockTaskbar({dockTaskbarEffective:'DOCK_HIDE_PENDING',dockTaskbarDesired:'dock',dockTaskbarActual:'VISIBLE',dockTaskbarPending:true});
  assert.equal(control.value,'dock');assert.equal(control.disabled,false);
  assert.match($('taskbarTakeoverStatus').textContent,/Yêu cầu:/);
  context.syncDockTaskbar({dockTaskbarEffective:'DOCK_HIDDEN_ACTIVE',dockTaskbarDesired:'dock',dockTaskbarActual:'HIDDEN'});
  assert.equal(control.value,'dock');assert.equal(control.disabled,false);
  for (const [mode,actual] of [['DOCK_PARKED_ACTIVE','AUTO_HIDE_PARKED'],['DOCK_EDGE_PEEK','VISIBLE']]){
    context.syncDockTaskbar({dockTaskbarEffective:mode,dockTaskbarDesired:'dock',dockTaskbarActual:actual,dockTaskbarHidden:false});
    assert.equal(control.value,'dock');
    assert.equal(control.disabled,false);
    assert.doesNotMatch($('taskbarTakeoverStatus').textContent,/Yêu cầu:/);
  }
  context.syncDockTaskbar({dockTaskbarEffective:'RESTORING',dockTaskbarActual:'VISIBLE'});
  assert.equal(control.value,'dock');assert.equal(control.disabled,false);
  context.syncDockSettings({dockTaskbarEffective:'RESTORED_BY_WINDOWS',dockTaskbarDesired:'dock',dockTaskbarActual:'VISIBLE',
    dockTaskbarActualLabel:'Đang hiện',dockTaskbarWindows:{visible:2,total:2,parked:0},dockTaskbarMessage:'Restored'});
  context.syncDockTaskbar({dockTaskbarHidden:true,dockTaskbarPending:true}); // stale legacy fields
  assert.equal(control.value,'dock');assert.equal(control.disabled,false);
  assert.match($('taskbarTakeoverStatus').textContent,/2\/2.*Restored/);
  assert.equal($('dockTaskbarRetry').hidden,false);
  $('dockTaskbarRetry').onclick();
  assert.deepEqual(JSON.parse(JSON.stringify(messages.pop())),{t:'dockTaskbar',hidden:true});
  context.syncDockTaskbar({dockTaskbarEffective:'RESTORED_BY_WATCHDOG',dockTaskbarDesired:'windows',dockTaskbarActual:'VISIBLE'});
  assert.equal(control.value,'windows');
  assert.equal($('dockTaskbarRetry').hidden,true);
  context.syncDockSettings({dock:{enabled:false},dockTaskbarHidden:false});
  assert.equal(control.disabled,true);
});
test('compact status and metrics preserve per-display options',()=>{
  const main={id:'main',name:'Main',w:2560,h:1080,dpi:96,primary:true};
  const side={id:'side',name:'Side',w:1080,h:1920,dpi:120,primary:false};
  context.syncDockSettings({dockDisplays:[main,side],dock:{utilityProfiles:{side:{rows:1,gap:8}}}});
  $('utilityDisplay').listeners.change({target:{value:'side'}});
  $('utilityGap').value='4';$('utilityGap').listeners.change();
  let m=messages.pop();assert.equal(m.display,'side');assert.equal(m.profile.rows,1);assert.equal(m.profile.gap,4);
  $('utilityMetrics').checked=true;$('utilityMetricGPU').checked=false;$('utilityMetricInterval').value='5';
  $('utilityMetrics').listeners.change();
  m=messages.pop();assert.equal(m.display,'side');assert.equal(m.profile.metrics.enabled,true);
    assert.equal(m.profile.metrics.mask,27);assert.equal(m.profile.metrics.interval,5);
    $('utilityMetricMode').checked=true;$('utilityMetricMode').listeners.change();
    assert.equal(messages.pop().profile.metrics.mode,'inline');
    $('utilityMetricMode').checked=false;$('utilityMetricMode').listeners.change();
    m=messages.pop();assert.equal(m.profile.metrics.mode,'popover');assert.equal(m.profile.metrics.enabled,true);
  $('utilityDisplay').listeners.change({target:{value:'main'}});
  assert.equal($('utilityMetrics').checked,false);
});
test('reset buttons send only the chosen appearance or display layout',()=>{
  const main={id:'main',name:'Main',w:1920,h:1080,dpi:96,primary:true};
  context.syncDockSettings({dockDisplays:[main,{...main,id:'vertical',primary:false}],dock:{
    appearance:{position:'right',theme:'dark'},stageStyles:{main:{scale:135},vertical:{scale:80}}
  }});
  $('dockLookReset').listeners.click();
  const dock=messages.pop();
  assert.deepEqual(Object.keys(dock).sort(),['appearance','t']);
  assert.equal(dock.appearance.theme,'auto');
  assert.equal($('dockLook_position').value,'bottom');
  $('stageStyleDisplay').listeners.change({target:{value:'vertical'}});
  $('stageStyleReset').listeners.click();
  const stage=messages.pop();
  assert.equal(stage.display,'vertical');
  assert.equal(stage.style.scale,100);
  assert.equal($('stageStyle_scale').value,'100');
  $('stageStyleDisplay').listeners.change({target:{value:'main'}});
  assert.equal($('stageStyle_scale').value,'135');
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
test('native Settings starts without a live iframe and recovery cannot reopen it',()=>{
  const from=html.indexOf('function updateFoot()'),to=html.indexOf('function renderHead()',from);
  assert(from>=0&&to>from);
  for(const previewRecovery of [false,true]){
    const nodes=new Map();
    const element=id=>{
      if(!nodes.has(id)){
        const attrs=new Map(),listeners={};
        nodes.set(id,{listeners,setAttribute:(k,v)=>attrs.set(k,v),removeAttribute:k=>attrs.delete(k),
          hasAttribute:k=>attrs.has(k),getAttribute:k=>attrs.get(k),addEventListener:(k,v)=>{listeners[k]=v;}});
      }return nodes.get(id);
    };
    const preview=element('preview');
    const ctx=vm.createContext({$:element,frame:preview,previewRecovery,livePreview:false,host:{},
      state:{theme:'beach',registry:{}},t:k=>k,themeById:()=>({page:'flat.html',id:'beach'}),thumbUrl:()=>'beach.png'});
    vm.runInContext(html.slice(from,to),ctx);
    ctx.refreshThemePreview();
    assert.equal(preview.getAttribute('src'),undefined);
    assert.equal(element('previewStill').getAttribute('src'),'beach.png');
    element('previewToggle').listeners.click();
    assert.equal(preview.getAttribute('src'),previewRecovery?undefined:'flat.html?preview=1');
    assert.equal(element('previewToggle').hidden,previewRecovery);
    ctx.setLivePreview(false);
    assert.equal(preview.getAttribute('src'),undefined);
    assert.equal(element('previewStill').hidden,false);
  }
});
console.log('TOTAL: '+count+' PASS / 0 FAIL');
