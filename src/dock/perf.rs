//! Bounded diagnostics for an opt-in isolated benchmark. The timestamps measure
//! client paint and submission, not the eventual DWM scan-out.
use std::{cell::RefCell, collections::{HashMap,VecDeque},fs,path::PathBuf,time::{Instant,SystemTime,UNIX_EPOCH}};

const MAX_ROWS: usize = 8192;
type Clock = (usize,Instant,Instant);
struct Sample {
    output:usize,
    elapsed_us:u64,
    gap_us:Option<u64>,
    from_tick_us:u64,
    paint_us:u64,
}
struct Capture {
    path:PathBuf,
    started:Instant,
    last:HashMap<usize,Clock>,
    next_output:usize,
    samples:VecDeque<Sample>,
    total:u64,
}
thread_local! {static CAPTURE:RefCell<Option<Capture>>=const{RefCell::new(None)};}

pub fn start() {
    let Some(profile)=crate::trial::data_dir() else{return;};
    if !crate::trial::benchmark_active(){return;}
    let stamp=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let path=profile.join(format!("dock-frames-{}-{stamp}.csv",std::process::id()));
    CAPTURE.with(|slot|slot.replace(Some(Capture{path,started:Instant::now(),last:HashMap::new(),
        next_output:0,samples:VecDeque::with_capacity(MAX_ROWS),total:0})));
}

pub fn submitted(window:usize,tick:Instant,paint:Instant) {
    CAPTURE.with(|slot|{
        let mut slot=slot.borrow_mut();
        let Some(c)=slot.as_mut() else{return;};
        if c.last.get(&window).is_some_and(|(_,previous_tick,_)|*previous_tick==tick){return;}
        let done=Instant::now();
        let (output,gap_us)=if let Some((output,prior_tick,prior))=c.last.get(&window) {
            // The first frame of a new animation burst has no interval.
            let gap=(tick.duration_since(*prior_tick).as_millis()<180).then(||done.duration_since(*prior).as_micros() as u64);
            (*output,gap)
        } else {
            c.next_output+=1;
            (c.next_output,None)
        };
        c.last.insert(window,(output,tick,done));
        if c.samples.len()==MAX_ROWS {c.samples.pop_front();}
        c.samples.push_back(Sample{output,elapsed_us:done.duration_since(c.started).as_micros() as u64,gap_us,
            from_tick_us:done.duration_since(tick).as_micros() as u64,paint_us:done.duration_since(paint).as_micros() as u64});
        c.total+=1;
    });
}

pub fn active()->bool {CAPTURE.with(|slot|slot.borrow().is_some())}

pub fn forget(window:usize){CAPTURE.with(|slot|{
    if let Some(c)=slot.borrow_mut().as_mut(){c.last.remove(&window);}
});}

pub fn finish() {
    use std::fmt::Write;
    let Some(c)=CAPTURE.with(|slot|slot.take()) else{return;};
    let mut csv=String::from("sequence,output,elapsed_us,submission_gap_us,tick_to_submission_us,paint_us\n");
    let first=c.total-c.samples.len() as u64+1;
    for (i,s) in c.samples.into_iter().enumerate(){
        let gap=s.gap_us.map_or_else(String::new,|gap|gap.to_string());
        let _=writeln!(csv,"{},{},{},{},{},{}",first+i as u64,s.output,s.elapsed_us,gap,s.from_tick_us,s.paint_us);
    }
    if let Err(error)=fs::write(c.path,csv){crate::log!("Dock benchmark trace: {error}");}
}
