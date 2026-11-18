(function(root){
  'use strict';
  const NO_REPEAT=0x4000;
  const named={Space:32,Enter:13,Tab:9,Backspace:8,Delete:46,Insert:45,Home:36,End:35,PageUp:33,PageDown:34,Left:37,Up:38,Right:39,Down:40};
  function keyLabel(vk){if(vk>=65&&vk<=90||vk>=48&&vk<=57)return String.fromCharCode(vk);if(vk>=96&&vk<=105)return 'Num'+(vk-96);if(vk>=112&&vk<=135)return 'F'+(vk-111);return Object.keys(named).find(k=>named[k]===vk)||'';}
  function valid(mods,vk){return !!(mods&7)&&!(mods&~(7|NO_REPEAT))&&vk!==123&&!!keyLabel(vk);}
  function label(mods,vk){return [(mods&2)&&'Ctrl',(mods&1)&&'Alt',(mods&4)&&'Shift',keyLabel(vk)].filter(Boolean).join('+');}
  function parse(text){const parts=String(text).trim().split('+').map(x=>x.trim());let mods=NO_REPEAT,vk=0;for(const part of parts){const p=part.toLowerCase();if(['ctrl','control'].includes(p))mods|=2;else if(p==='alt')mods|=1;else if(p==='shift')mods|=4;else{if(vk)return null;const name=Object.keys(named).find(k=>k.toLowerCase()===p);if(name)vk=named[name];else if(/^[a-z0-9]$/i.test(part))vk=part.toUpperCase().charCodeAt(0);else if(/^num[0-9]$/i.test(part))vk=96+Number(part.slice(3));else if(/^f(?:[1-9]|1[0-9]|2[0-4])$/i.test(part))vk=111+Number(part.slice(1));else return null;}}return valid(mods,vk)?{mods,vk}:null;}
  function fromEvent(e){let vk=0;if(/^Key[A-Z]$/.test(e.code))vk=e.code.charCodeAt(3);else if(/^Digit[0-9]$/.test(e.code))vk=48+Number(e.code.slice(5));else if(/^Numpad[0-9]$/.test(e.code))vk=96+Number(e.code.slice(6));else if(/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(e.code))vk=111+Number(e.code.slice(1));else vk=named[e.code]||named[e.key]||named[String(e.key).replace(/^Arrow/,'')]||0;const mods=NO_REPEAT|(e.ctrlKey?2:0)|(e.altKey?1:0)|(e.shiftKey?4:0);return !e.metaKey&&valid(mods,vk)?{mods,vk}:null;}
  const api={valid,parse,label,keyLabel,fromEvent};if(typeof module!=='undefined'&&module.exports)module.exports=api;else root.ShortcutModel=api;
})(typeof window==='undefined'?globalThis:window);
