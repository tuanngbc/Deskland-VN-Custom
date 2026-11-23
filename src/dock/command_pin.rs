//! Command pins share the legacy pin list without changing EXE/LNK entries.
use std::{path::Path,ptr};
use windows_sys::Win32::{Foundation::LocalFree,UI::Shell::CommandLineToArgvW};

const PREFIX:&str="deskland-command:v1:";
pub(super) struct Launch {
    pub exe:String,
    pub parameters:String,
    pub directory:String,
    pub label:String,
}
fn absolute_exe(path:&str)->bool{
    Path::new(path).is_absolute()&&Path::new(path).extension().and_then(|v|v.to_str())
        .is_some_and(|v|v.eq_ignore_ascii_case("exe"))
}
fn split(line:&str)->Option<(&str,&str)>{
    if let Some(quoted)=line.strip_prefix('"'){
        let (exe,tail)=quoted.split_once('"')?;
        return (absolute_exe(exe)&&tail.chars().next().is_none_or(char::is_whitespace)).then_some((exe,tail.trim_start()));
    }
    line.char_indices().map(|(i,_)|i).chain([line.len()]).find_map(|i|{
        let tail=&line[i..];
        (tail.chars().next().is_none_or(char::is_whitespace)&&absolute_exe(&line[..i]))
            .then(||(&line[..i],tail.trim_start()))
    })
}
fn first_argument(parameters:&str)->Option<String>{
    if parameters.is_empty(){return None;}
    let mut argc=0;
    let wide=crate::desktop::wide(&format!("command {parameters}"));
    unsafe{
        let args=CommandLineToArgvW(wide.as_ptr(),&mut argc);
        if args.is_null(){return None;}
        let result=(argc>1).then(||{
            let first=*args.add(1);
            let len=(0..).take_while(|&i|*first.add(i)!=0).count();
            String::from_utf16_lossy(std::slice::from_raw_parts(first,len))
        });
        LocalFree(args.cast());
        result
    }
}
fn parse(line:&str,label:&str)->Option<Launch>{
    if line.len()>4096||line.chars().any(char::is_control)||label.chars().any(char::is_control){return None;}
    let (exe,parameters)=split(line.trim())?;
    let executable=Path::new(exe);
    let script=first_argument(parameters).filter(|arg|{
        let path=Path::new(arg);
        path.is_absolute()&&path.extension().and_then(|v|v.to_str())
            .is_some_and(|v|v.eq_ignore_ascii_case("py")||v.eq_ignore_ascii_case("pyw"))
    });
    let subject=script.as_deref().map(Path::new).unwrap_or(executable);
    let directory=subject.parent()?.to_str()?.to_owned();
    let label=if label.trim().is_empty(){subject.file_stem()?.to_str()?.to_owned()}
        else{label.trim().chars().take(80).collect()};
    Some(Launch{exe:exe.into(),parameters:parameters.into(),directory,label})
}
pub(super) fn encode(line:&str,label:&str)->Result<String,&'static str>{
    let Some(launch)=parse(line,label)else{return Err("Nhập đường dẫn đầy đủ đến tệp .exe, rồi thêm tham số nếu cần.");};
    if !Path::new(&launch.exe).is_file(){return Err("Không tìm thấy tệp .exe tại đường dẫn đã nhập.");}
    Ok(key(&launch))
}
fn key(launch:&Launch)->String{
    // The existing pin membership check ignores ASCII case. Hex encoding
    // keeps the case of parameters significant without changing path pins.
    use std::fmt::Write;
    let mut key=PREFIX.to_owned();
    let value=serde_json::json!([launch.exe,launch.parameters,launch.directory,launch.label]);
    for byte in value.to_string().bytes(){let _=write!(key,"{byte:02x}");}
    key
}
pub(super) fn decode(value:&str)->Option<Launch>{
    let bytes=value.strip_prefix(PREFIX)?.as_bytes();
    if bytes.len()%2!=0||bytes.len()>30000{return None;}
    let digit=|b|match b{b'0'..=b'9'=>Some(b-b'0'),b'a'..=b'f'=>Some(b-b'a'+10),_=>None};
    let data=bytes.chunks_exact(2).map(|c|Some(digit(c[0])?*16+digit(c[1])?)).collect::<Option<Vec<_>>>()?;
    let [exe,parameters,directory,label]=serde_json::from_slice::<[String;4]>(&data).ok()?;
    (absolute_exe(&exe)&&Path::new(&directory).is_absolute()&&[&exe,&parameters,&directory,&label].into_iter()
        .all(|s|!s.chars().any(char::is_control))).then_some(Launch{exe,parameters,directory,label})
}
pub(super) unsafe fn open(value:&str)->isize{
    use windows_sys::Win32::UI::{Shell::ShellExecuteW,WindowsAndMessaging::SW_SHOWNORMAL};
    use crate::desktop::wide;
    let command=decode(value);
    let parameters=command.as_ref().map(|c|wide(&c.parameters));
    let directory=command.as_ref().map(|c|wide(&c.directory));
    let exe=wide(command.as_ref().map_or(value,|c|&c.exe));
    ShellExecuteW(ptr::null_mut(),wide("open").as_ptr(),exe.as_ptr(),
        parameters.as_deref().map_or(ptr::null(),|v|v.as_ptr()),
        directory.as_deref().map_or(ptr::null(),|v|v.as_ptr()),SW_SHOWNORMAL) as isize
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn python_script_keeps_arguments_and_uses_script_directory(){
        let launch=parse(r#"E:\PandaTV\_Studio\venv\Scripts\pythonw.exe "E:\PandaTV\_Studio\main.py" --Title "A & B""#,"").unwrap();
        let restored=decode(&key(&launch)).unwrap();
        assert_eq!(restored.exe,r"E:\PandaTV\_Studio\venv\Scripts\pythonw.exe");
        assert_eq!(restored.directory,r"E:\PandaTV\_Studio");
        assert_eq!(restored.parameters,r#""E:\PandaTV\_Studio\main.py" --Title "A & B""#);
        assert_eq!(restored.label,"main");
        let other=key(&Launch{parameters:restored.parameters.replace("--Title","--title"),..restored});
        assert!(!other.eq_ignore_ascii_case(&key(&launch)));
    }
    #[test]
    #[ignore = "starts a native process fixture; run individually in CI"]
    fn native_shell_launch_passes_arguments_and_working_directory(){
        use std::time::{Duration,Instant};
        assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(),Ok("true"));
        let directory=std::env::temp_dir().join(format!("deskland command pin {}",std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let proof=directory.join("argv check.txt");
        let fixture=directory.join("fixture command.ps1");
        std::fs::write(&fixture,r#"if ($args.Count -eq 2 -and $args[0] -ceq 'A & B' -and $args[1] -ceq 'CaseSensitive' -and (Get-Location).Path -ieq $PSScriptRoot) {
    $temp = Join-Path $PSScriptRoot 'argv partial.txt'
    [IO.File]::WriteAllText($temp, 'fixture reached')
    [IO.File]::Move($temp, (Join-Path $PSScriptRoot 'argv check.txt'))
}"#).unwrap();
        let executable=Path::new(&std::env::var_os("SystemRoot").unwrap()).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        let line=format!("\"{}\" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \"{}\" \"A & B\" CaseSensitive",
            executable.display(),fixture.display());
        let launch=Launch{directory:directory.to_string_lossy().into_owned(),..parse(&line,"Fixture").unwrap()};
        assert!(unsafe{open(&key(&launch))}>32);
        let start=Instant::now();
        while !proof.exists()&&start.elapsed()<Duration::from_secs(8){std::thread::sleep(Duration::from_millis(25));}
        assert_eq!(std::fs::read(&proof).unwrap(),b"fixture reached");
        std::fs::remove_file(proof).unwrap();
        std::fs::remove_file(fixture).unwrap();
        // Explorer returns before the process exits. Windows keeps the
        // working directory locked until the child has fully shut down.
        let cleanup=Instant::now();
        loop{
            match std::fs::remove_dir(&directory){
                Ok(())=>break,
                Err(err) if matches!(err.raw_os_error(),Some(5|32))&&cleanup.elapsed()<Duration::from_secs(4)=>
                    std::thread::sleep(Duration::from_millis(25)),
                Err(err)=>panic!("fixture directory locked: {err}"),
            }
        }
    }
    #[test]
    fn validates_quoted_and_unquoted_paths_with_spaces(){
        for line in [
            r#""C:\Program Files\Foo\foo.EXE" /x "a b""#,
            r#"C:\Program Files\Foo\foo.EXE /x "a b""#,
        ]{
            let launch=parse(line,"Tên mới").unwrap();
            assert_eq!(launch.exe,r"C:\Program Files\Foo\foo.EXE");
            assert_eq!(launch.parameters,r#"/x "a b""#);
            assert_eq!(launch.label,"Tên mới");
        }
        for line in [r#"foo.exe"#,r#""C:\Foo.exe"x"#,"foo\0.exe",r#"cmd /c foo.exe"#]{
            assert!(parse(line,"").is_none());
        }
        assert!(decode(r"C:\legacy\existing.lnk").is_none());
    }
}
