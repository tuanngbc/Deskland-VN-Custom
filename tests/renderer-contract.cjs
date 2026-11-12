'use strict';
const fs=require('node:fs'), path=require('node:path'), vm=require('node:vm'), assert=require('node:assert/strict');
const world=require('../prototype/world-contract.js');
let passed=0;
function test(name,fn){fn();passed++;console.log('AUTOMATED PASS '+name);}
test('deterministic random seed',()=>{world.seed(71);const a=Array.from({length:1000},()=>world.random());world.seed(71);assert.deepEqual(a,Array.from({length:1000},()=>world.random()));assert(a.every(n=>n>=0&&n<1));});
test('independent random seeds',()=>{world.seed(1);const a=world.random();world.seed(2);assert.notEqual(a,world.random());});
test('negative screen origin maps to world',()=>assert.deepEqual(world.localRect([-1000,-600,-920,-510],{x:-1080,y:-685}),[80,85,160,175]));
test('mixed DPI physical coordinates',()=>{for(const ratio of [1,1.25,1.5,1.75,2,3]){const c=world.cssRect([300,150,390,270],ratio);assert.equal(c.x*ratio,300);assert.equal(c.h*ratio,120);}});
test('global clock survives reload and viewport changes',()=>{const epoch=Date.now()-5000;world.configure({version:1,epoch,mode:'mirror'});const a=world.seconds(performance.now(),0);assert(a>=5&&a<6);world.configure({version:1,epoch,mode:'panorama'});assert(Math.abs(world.seconds(performance.now(),0)-a)<.02);});
for(const file of ['flat.html','meadow.html','settings.html']) test(file+' script syntax',()=>{const html=fs.readFileSync(path.join(__dirname,'../prototype',file),'utf8');for(const m of html.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g))new vm.Script(m[1],{filename:file});});
test('all ready themes contain shared contract',()=>{const registry=JSON.parse(fs.readFileSync(path.join(__dirname,'../prototype/themes.json')));for(const t of registry.themes){if(t.status==='ready'){const html=fs.readFileSync(path.join(__dirname,'../prototype',t.page),'utf8');assert(html.includes('world-contract.js'));assert(html.includes('DeskWorld.configure(m.contract)'));}else assert(!t.page);}});
test('localized multi-monitor controls',()=>{const en=require('../prototype/i18n/en.json').ui,zh=require('../prototype/i18n/zh-CN.json').ui;for(const key of ['monitorTitle','mode_mirror','mode_independent','mode_panorama','applyMulti','restoreMulti','pauseMonitor']){assert(en[key]);assert(zh[key]);}});
console.log(`TOTAL: ${passed} PASS / 0 FAIL`);
