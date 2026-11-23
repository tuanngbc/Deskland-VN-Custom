(function(){
  'use strict';
  let current='wallpaper';
  const fallback={dock:['Dock & Stage Manager','Quản lý các thành phần trong cùng không gian làm việc.'],wallpaper:['Hình nền','Chọn một khung cảnh cho ngày của bạn.'],monitors:['Màn hình','Bố trí không gian làm việc theo cách của bạn.'],weather:['Thời tiết & ánh sáng','Đưa nhịp điệu ngoài trời vào màn hình.'],clock:['Đồng hồ','Chỉnh từng dòng chữ, theo đúng ý bạn.'],shortcuts:['Phím tắt & tiện ích','Mở nhanh, chụp gọn, làm việc liền mạch.'],system:['Hệ thống','Những thiết lập chung cho không gian của bạn.']};
  function copy(){const key='nav_'+current,sub='navSub_'+current;$('pageTitle').textContent=t(key)===key?fallback[current][0]:t(key);$('pageSubtitle').textContent=t(sub)===sub?fallback[current][1]:t(sub);}
  for(const button of document.querySelectorAll('.nav-item'))button.addEventListener('click',()=>{
    current=button.dataset.page;
    if(current!=='wallpaper')setLivePreview(false);
    for(const page of document.querySelectorAll('.settings-page'))page.hidden=page.id!=='page-'+current;
    for(const nav of document.querySelectorAll('.nav-item')){if(nav===button)nav.setAttribute('aria-current','page');else nav.removeAttribute('aria-current');}
    copy();$('pageViewport').scrollTop=0;requestAnimationFrame(()=>{layoutPreview();renderMonitors();});
  });
  const dockTabs=[...document.querySelectorAll('.dock-groups [role=tab]')];
  function selectDockTab(index,focus){
    dockTabs.forEach((tab,i)=>{
      const selected=i===index;
      tab.setAttribute('aria-selected',String(selected));
      tab.tabIndex=selected?0:-1;
      $(tab.getAttribute('aria-controls')).hidden=!selected;
    });
    if(focus)dockTabs[index].focus();
  }
  dockTabs.forEach((tab,i)=>{
    tab.addEventListener('click',()=>selectDockTab(i,false));
    tab.addEventListener('keydown',e=>{
      const step={ArrowLeft:-1,ArrowRight:1}[e.key];
      const next=e.key==='Home'?0:e.key==='End'?dockTabs.length-1:
        step?(i+step+dockTabs.length)%dockTabs.length:-1;
      if(next<0)return;
      e.preventDefault();selectDockTab(next,true);
    });
  });
  const normalize=s=>String(s).normalize('NFD').replace(/[\u0300-\u036f]/g,'').replace(/đ/g,'d').toLowerCase();
  window.filterSettingsThemes=()=>{const query=normalize($('themeSearch').value);let count=0;for(const card of $('themes').children){card.hidden=!normalize(card.textContent).includes(query);if(!card.hidden)count++;}$('emptySearch').hidden=count>0;};
  $('themeSearch').addEventListener('input',window.filterSettingsThemes);
  function appearance(value){if(value==='system')delete document.documentElement.dataset.appearance;else document.documentElement.dataset.appearance=value;if(host)send({t:'settingsAppearance',value});}
  let preference='system';try{preference=localStorage.getItem('tuanpc-settings-appearance')||'system';}catch(_){}
  if(!['system','light','dark'].includes(preference))preference='system';
  $('settingsAppearance').value=preference;appearance(preference);
  $('settingsAppearance').addEventListener('change',e=>{appearance(e.target.value);try{localStorage.setItem('tuanpc-settings-appearance',e.target.value);}catch(_){}});
  window.addEventListener('settings-ready',()=>{copy();window.filterSettingsThemes();});
  new MutationObserver(copy).observe(document.documentElement,{attributes:true,attributeFilter:['lang']});
  document.addEventListener('keydown',e=>{
    if(e.key!=='Tab')return;
    const modal=Array.from(document.querySelectorAll('.modal')).find(x=>!x.hidden);if(!modal)return;
    const items=Array.from(modal.querySelectorAll('button:not(:disabled),input,select,a[href],[tabindex="0"]')).filter(x=>x.getClientRects().length);
    const first=items[0],last=items[items.length-1];if(!first)return;
    if(e.shiftKey&&document.activeElement===first){e.preventDefault();last.focus();}else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first.focus();}
  });
  copy();window.filterSettingsThemes();
})();
