'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const model=require('../prototype/shortcut-model.js');let count=0;
function test(name,fn){fn();count++;console.log('PASS '+name);}
test('all single and combined modifiers round trip without preset select options',()=>{
  for(let mods=1;mods<=7;mods++)for(const vk of [8,9,13,32,33,34,35,36,37,38,39,40,45,46,48,57,65,90,96,105,112,122,124,135]){
    const normalized=mods|0x4000;assert.deepEqual(model.parse(model.label(normalized,vk)),{mods:normalized,vk});
  }
});
test('physical keyboard codes remain stable with Vietnamese input and numpad',()=>{
  for(const [code,key,vk]of [['KeyK','ķ',75],['Digit8','*',56],['Numpad8','ArrowUp',104],['F24','F24',135],['ArrowLeft','ArrowLeft',37]])assert.deepEqual(model.fromEvent({code,key,ctrlKey:true,altKey:true,shiftKey:false}),{mods:16387,vk});
});
test('invalid and reserved shortcuts are rejected rather than silently replaced by defaults',()=>{
  for(const input of ['','K','Ctrl','Ctrl+K+L','Win+K','Ctrl+F12','Ctrl+F25','Ctrl+Unknown','Ctrl++'])assert.equal(model.parse(input),null,input);
  assert.equal(model.fromEvent({code:'KeyK',key:'k',ctrlKey:true,metaKey:true}),null);
});
test('native WebView and accessibility input work without a physical code',()=>{
  assert.deepEqual(model.fromEvent({code:'',key:'k',keyCode:75,ctrlKey:true,altKey:true}),{mods:16387,vk:75});
  assert.deepEqual(model.fromEvent({code:'Unidentified',key:'K',ctrlKey:true,altKey:true}),{mods:16387,vk:75});
  assert.deepEqual(model.fromEvent({key:'0',location:3,shiftKey:true}),{mods:16388,vk:96});
});
test('typed shortcuts accept casing, whitespace and common named keys',()=>{
  assert.deepEqual(model.parse(' alt + k '),{mods:16385,vk:75});
  assert.deepEqual(model.parse('Control+Shift+PageDown'),{mods:16390,vk:34});
  assert.deepEqual(model.parse('shift+num0'),{mods:16388,vk:96});
});
test('settings IDs are unique and all fixed controls exist',()=>{
  const html=fs.readFileSync(path.join(__dirname,'../prototype/settings.html'),'utf8');
  const markup=html.slice(0,html.indexOf('<script src="thumbs-data.js">'));
  const ids=[...markup.matchAll(/\bid="([^"]+)"/g)].map(m=>m[1]);assert.equal(ids.length,new Set(ids).size);
  const generated=new Set(['share', 'toolMods_', 'toolKey_']);
  for(const match of html.matchAll(/\$\('([A-Za-z][A-Za-z0-9_-]*)'\)/g))assert(ids.includes(match[1])||generated.has(match[1]),'Missing control '+match[1]);
  for(const name of ['wallpaper','monitors','weather','clock','shortcuts','system'])assert(ids.includes('page-'+name));
});
test('new UI strings are available in Vietnamese and English',()=>{
  const html=fs.readFileSync(path.join(__dirname,'../prototype/settings.html'),'utf8');
  for(const code of ['vi','en']){const ui=require('../prototype/i18n/'+code+'.json').ui;for(const m of html.matchAll(/data-i18n="([A-Za-z0-9_]+)"/g))assert(ui[m[1]],code+': '+m[1]);}
});
console.log('TOTAL: '+count+' PASS / 0 FAIL');
