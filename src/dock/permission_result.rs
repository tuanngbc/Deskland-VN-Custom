//! Shared WinRT permission completion rules. Pending, canceled and failed
//! operations must never call GetResults or turn a failure into permission.
use std::time::Duration;
pub(super) const TIMEOUT:i32=0x800705b4u32 as i32;
pub(super) fn outcome(status:Result<i32,i32>,elapsed:Duration,results:impl FnOnce()->Result<i32,i32>,error:impl FnOnce()->Result<i32,i32>)->Option<Result<i32,i32>>{
    match status{
        Ok(0) if elapsed<Duration::from_secs(30)=>None,
        Ok(0)=>Some(Err(TIMEOUT)),
        Ok(1)=>Some(results()),
        Ok(2)=>Some(Err(0x80004004u32 as i32)),
        Ok(3)=>Some(Err(match error(){Ok(code)|Err(code)=>if code<0{code}else{0x8000ffffu32 as i32}})),
        Ok(_)=>Some(Err(0x8000ffffu32 as i32)),
        Err(error)=>Some(Err(error)),
    }
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn completion_preserves_failure_and_only_reads_the_matching_result(){
        let no_results=||panic!("only Completed may call GetResults");
        let no_error=||panic!("only Error may call ErrorCode");
        assert_eq!(outcome(Ok(0),Duration::from_secs(2),no_results,no_error),None);
        assert_eq!(outcome(Ok(0),Duration::from_secs(30),no_results,no_error),Some(Err(TIMEOUT)));
        for result in [Ok(0),Ok(1),Ok(2),Ok(3),Ok(4),Err(0x80073d54u32 as i32)]{assert_eq!(outcome(Ok(1),Duration::ZERO,||result,no_error),Some(result));}
        assert_eq!(outcome(Ok(2),Duration::ZERO,no_results,no_error),Some(Err(0x80004004u32 as i32)));
        for code in [0x80070005u32 as i32,0x80073d54u32 as i32]{
            assert_eq!(outcome(Ok(3),Duration::ZERO,no_results,||Ok(code)),Some(Err(code)));
            assert_eq!(outcome(Ok(3),Duration::ZERO,no_results,||Err(code)),Some(Err(code)));
        }
        assert_eq!(outcome(Ok(3),Duration::ZERO,no_results,||Ok(0)),Some(Err(0x8000ffffu32 as i32)));
        assert_eq!(outcome(Ok(99),Duration::ZERO,no_results,no_error),Some(Err(0x8000ffffu32 as i32)));
        assert_eq!(outcome(Err(0x80004005u32 as i32),Duration::ZERO,no_results,no_error),Some(Err(0x80004005u32 as i32)));
    }
}
