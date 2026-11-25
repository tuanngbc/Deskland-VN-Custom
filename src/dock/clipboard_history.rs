//! In-memory clipboard history adapted from FluidWin appunti.h.
//! Copyright (c) 2026 Clooudy (MIT). No clipboard content is logged or saved.
use std::{cell::RefCell, collections::VecDeque, ffi::c_void, ptr, sync::{Mutex,atomic::{AtomicBool,Ordering}}};
use windows_sys::Win32::{Foundation::*, System::{DataExchange::*,Memory::*,Threading::{TrySubmitThreadpoolCallback,PTP_CALLBACK_INSTANCE}},
    UI::WindowsAndMessaging::*};
use super::control_center::{Card,Command,View};

const TEXT:u32=13; // CF_UNICODETEXT
const DIB:u32=8;   // CF_DIB
const MAX_TEXT:usize=40_002;
const MAX_IMAGE:usize=2*1024*1024;
const MAX_HISTORY:usize=4*1024*1024;
pub(super) const READY:u32=WM_APP+119;

#[derive(PartialEq,Eq)]
struct Value{format:u32,bytes:Vec<u8>}
impl Value{
    fn label(&self)->String{
        if self.format==DIB{
            let w=i32::from_le_bytes(self.bytes[4..8].try_into().unwrap());
            let h=i32::from_le_bytes(self.bytes[8..12].try_into().unwrap()).unsigned_abs();
            return format!("{w} × {h}");
        }
        let utf16=self.bytes.chunks_exact(2).map(|p|u16::from_le_bytes([p[0],p[1]])).take_while(|&p|p!=0);
        char::decode_utf16(utf16).take(100).map(|s|s.unwrap_or('�')).map(|c|if c.is_control(){' '}else{c}).collect()
    }
}
struct State{
    hwnd:HWND,
    generation:u64, sequence:Option<u32>, next_id:u64, capacity:usize,
    values:VecDeque<(u64,Value)>
}
impl Default for State{
    fn default()->Self{Self{hwnd:ptr::null_mut(),generation:1,sequence:None,next_id:0,capacity:0,values:VecDeque::new()}}
}
impl State{
    fn trim(&mut self){
        while self.values.len()>self.capacity||self.values.iter().map(|(_,v)|v.bytes.len()).sum::<usize>()>MAX_HISTORY{
            self.values.pop_back();
        }
    }
    fn insert(&mut self,value:Value){
        if let Some(i)=self.values.iter().position(|(_,v)|v==&value){self.values.remove(i);}
        self.next_id=self.next_id.wrapping_add(1).max(1);
        self.values.push_front((self.next_id,value));self.trim();
    }
}
thread_local!{static STATE:RefCell<State>=RefCell::new(State::default());}
struct Job{hwnd:usize,generation:u64,sequence:u32}
struct Completion{job:Job,result:Option<Value>}
static BUSY:AtomicBool=AtomicBool::new(false);
static DONE:Mutex<Option<Completion>>=Mutex::new(None);

unsafe fn selected(format:u32,cap:usize)->Option<Vec<u8>>{
    let data=GetClipboardData(format);
    if data.is_null(){return None;}
    let len=GlobalSize(data);
    if len==0||len>cap{return None;}
    let src=GlobalLock(data).cast::<u8>();
    if src.is_null(){return None;}
    let bytes=std::slice::from_raw_parts(src,len).to_vec();
    GlobalUnlock(data);
    Some(bytes)
}
unsafe fn extract(sequence:u32)->Option<Value>{
    let owner=GetClipboardOwner();
    if !owner.is_null(){
        let mut reply=0usize;
        if SendMessageTimeoutW(owner,WM_NULL,0,0,SMTO_ABORTIFHUNG|SMTO_ERRORONEXIT,120,&mut reply)==0{return None;}
    }
    if GetClipboardSequenceNumber()!=sequence||OpenClipboard(ptr::null_mut())==0{return None;}
    struct Close;
    impl Drop for Close{fn drop(&mut self){unsafe{CloseClipboard();}}}
    let _close=Close;
    let format=|name:&str|RegisterClipboardFormatW(crate::desktop::wide(name).as_ptr());
    let excluded=format("ExcludeClipboardContentFromMonitorProcessing");
    let history=format("CanIncludeInClipboardHistory");
    if excluded==0||history==0||IsClipboardFormatAvailable(excluded)!=0{return None;}
    if IsClipboardFormatAvailable(history)!=0{
        let Some(flag)=selected(history,4096)else{return None;};
        if flag.get(..4).is_none_or(|v|v==[0,0,0,0]){return None;}
    }
    if IsClipboardFormatAvailable(TEXT)!=0{
        let mut bytes=selected(TEXT,MAX_TEXT)?;
        let end=bytes.chunks_exact(2).position(|p|p==[0,0])?*2;
        if end==0{return None;}
        bytes.truncate(end+2);
        let value=Value{format:TEXT,bytes};
        return (!value.label().trim().is_empty()).then_some(value);
    }
    let bytes=selected(DIB,MAX_IMAGE)?;
    if bytes.len()<40{return None;}
    let get=|offset:usize| i32::from_le_bytes(bytes[offset..offset+4].try_into().unwrap());
    let w=get(4);let h=get(8).unsigned_abs();
    let bpp=u16::from_le_bytes(bytes[14..16].try_into().unwrap());
    if w<=0||h==0||w>4096||h>4096||!matches!(bpp,24|32)||get(16)!=0{return None;}
    let stride=(usize::try_from(w).ok()?*usize::from(bpp)).div_ceil(32)*4;
    let data_offset=usize::try_from(get(0)).ok()?;
    if data_offset<40||data_offset.checked_add(stride*h as usize)?>bytes.len(){return None;}
    Some(Value{format:DIB,bytes})
}
unsafe extern "system" fn collect(_:PTP_CALLBACK_INSTANCE,raw:*mut c_void){
    let job=*Box::from_raw(raw.cast::<Job>());
    let result=extract(job.sequence);
    let hwnd=job.hwnd as HWND;
    if let Ok(mut slot)=DONE.lock(){*slot=Some(Completion{job,result});}
    BUSY.store(false,Ordering::Release);
    PostMessageW(hwnd,READY,0,0);
}
pub(super) unsafe fn changed(hwnd:HWND){
    let sequence=GetClipboardSequenceNumber();
    STATE.with(|state|{
        let mut state=state.borrow_mut();
        if state.hwnd!=hwnd||state.sequence==Some(sequence)||BUSY.swap(true,Ordering::AcqRel){return;}
        state.sequence=Some(sequence);
        let raw=Box::into_raw(Box::new(Job{hwnd:hwnd as usize,generation:state.generation,sequence}));
        if TrySubmitThreadpoolCallback(Some(collect),raw.cast(),ptr::null())==0{
            drop(Box::from_raw(raw));BUSY.store(false,Ordering::Release);
        }
    });
}
pub(super) unsafe fn complete(hwnd:HWND){
    let completion=DONE.lock().ok().and_then(|mut slot|slot.take());
    if let Some(Completion{job,result:Some(value)})=completion{
        STATE.with(|s|{
            let mut s=s.borrow_mut();
            if s.hwnd==hwnd && s.generation==job.generation && GetClipboardSequenceNumber()==job.sequence{s.insert(value);}
        });
    }
    changed(hwnd);
}
pub(super) unsafe fn configure(hwnd:HWND,enabled:bool,limit:usize){
    STATE.with(|s|{
        let mut s=s.borrow_mut();
        if (!enabled&&s.hwnd.is_null())||(enabled&&s.hwnd==hwnd&&s.capacity==limit){return;}
        if !s.hwnd.is_null(){RemoveClipboardFormatListener(s.hwnd);}
        s.hwnd=ptr::null_mut();s.generation=s.generation.wrapping_add(1);s.sequence=None;
        if enabled&&AddClipboardFormatListener(hwnd)!=0{
            s.hwnd=hwnd;s.capacity=limit;
            // Start with the next user clipboard update.
            s.sequence=Some(GetClipboardSequenceNumber());s.trim();
        }else{s.values.clear();}
    });
    if let Ok(mut slot)=DONE.lock(){*slot=None;}
}
pub(super) unsafe fn copy(hwnd:HWND,id:u64)->bool{
    let value=STATE.with(|s|s.borrow().values.iter().find(|(i,_)|*i==id).map(|(_,v)|(v.format,v.bytes.clone())));
    let Some((format,bytes))=value else{return false;};
    let data=GlobalAlloc(GMEM_MOVEABLE,bytes.len());
    if data.is_null(){return false;}
    let buffer=GlobalLock(data).cast::<u8>();
    if buffer.is_null(){GlobalFree(data);return false;}
    ptr::copy_nonoverlapping(bytes.as_ptr(),buffer,bytes.len());
    GlobalUnlock(data);
    if OpenClipboard(hwnd)==0{GlobalFree(data);return false;}
    let copied=EmptyClipboard()!=0&&!SetClipboardData(format,data).is_null();
    CloseClipboard();
    if !copied{GlobalFree(data);}
    copied
}
pub(super) fn remove(id:Option<u64>){
    STATE.with(|s|{
        let mut s=s.borrow_mut();
        s.generation=s.generation.wrapping_add(1);
        if let Some(id)=id{s.values.retain(|(i,_)|*i!=id);}else{s.values.clear();}
    });
}
pub(super) fn view()->View{
    let text=|vi,en,zh|crate::launcher::localized(vi,en,zh);
    let mut cards=vec![Card{icon:None,title:text("Xóa lịch sử","Clear history","清除历史").into(),hint:String::new(),action:Some(Command::ClipboardRemove(None))}];
    let count=STATE.with(|s|{
        let s=s.borrow();
        for (id,value) in &s.values{
            let mut title=value.label();
            if value.format==DIB{title=format!("{} · {}",text("Ảnh","Image","图片"),title);}
            cards.push(Card{icon:None,title,hint:text("Sao chép lại","Copy again","再次复制").into(),action:Some(Command::ClipboardCopy(*id))});
            cards.push(Card{icon:None,title:text("Xóa mục trên","Delete above item","删除上一项").into(),hint:String::new(),action:Some(Command::ClipboardRemove(Some(*id)))});
        }
        s.values.len()
    });
    View{title:text("Lịch sử clipboard","Clipboard history","剪贴板历史").into(),
        hint:format!("{count} · {}",text("Chỉ lưu trong bộ nhớ","In memory only","仅保存在内存中")),cards,metrics:None}
}
pub(super) fn active()->bool{STATE.with(|s|!s.borrow().hwnd.is_null())}

#[cfg(test)]
#[test]
fn history_deduplicates_and_honors_count_and_byte_limits(){
    let mut s=State{capacity:5,..State::default()};
    for id in 0..8u8{
        s.insert(Value{format:TEXT,bytes:vec![id,0,0,0]});
    }
    assert_eq!(s.values.len(),5);
    assert_eq!(s.values.back().unwrap().1.bytes[0],3);
    s.insert(Value{format:TEXT,bytes:vec![3,0,0,0]});
    assert_eq!(s.values.len(),5);
    assert_eq!(s.values.front().unwrap().1.bytes[0],3);
    for id in 0..3u8{s.insert(Value{format:DIB,bytes:vec![id;MAX_IMAGE]});}
    assert_eq!(s.values.len(),2);
    assert_eq!(s.values.front().unwrap().1.bytes[0],2);
    assert_eq!(s.values.back().unwrap().1.bytes[0],1);
}
