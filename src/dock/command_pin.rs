//! Command pins share the legacy pin list without changing EXE/LNK entries.
use std::{path::Path,ptr};
use windows_sys::Win32::{Foundation::LocalFree,UI::Shell::CommandLineToArgvW};

const PREFIX:&str="deskland-command:v1:";
pub(crate) struct Launch {
    pub exe:String,
    pub parameters:String,
    pub directory:String,
    pub label:String,
}
fn absolute_exe(path:&str)->bool{
    Path::new(path).is_absolute()&&Path::new(path).extension().and_then(|v|v.to_str())
        .is_some_and(|v|v.eq_ignore_ascii_case("exe"))
}
fn unescape_path(path:&str)->String{
    path.replace(r"\_", "_").replace(r"\.", ".")
}
fn exe_token(path:&str)->bool{
    absolute_exe(path)||(path.contains(r"\.")&&absolute_exe(&unescape_path(path)))
}
fn split(line:&str)->Option<(&str,&str)>{
    if let Some(quoted)=line.strip_prefix('"'){
        let (exe,tail)=quoted.split_once('"')?;
        return (exe_token(exe)&&tail.chars().next().is_none_or(char::is_whitespace)).then_some((exe,tail.trim_start()));
    }
    line.char_indices().map(|(i,_)|i).chain([line.len()]).find_map(|i|{
        let tail=&line[i..];
        (tail.chars().next().is_none_or(char::is_whitespace)&&exe_token(&line[..i]))
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
#[derive(Debug)]
pub(crate) struct PinError {pub message:&'static str,pub suggestion:Option<String>}
fn corrected_path(path:&str,exists:&impl Fn(&str)->bool)->Option<String>{
    if !Path::new(path).is_absolute()||exists(path){return None;}
    let repaired=unescape_path(path);
    (repaired!=path&&exists(&repaired)).then_some(repaired)
}
fn quote(path:&str)->String{
    if path.chars().any(char::is_whitespace){format!("\"{path}\"")}else{path.into()}
}
fn suggest_repair(launch:&Launch,exists:&impl Fn(&str)->bool)->Option<String>{
    let repaired_exe=corrected_path(&launch.exe,exists);
    if !exists(&launch.exe)&&repaired_exe.is_none(){return None;}
    let (script,tail)=if let Some(value)=launch.parameters.strip_prefix('"'){
        value.split_once('"').unwrap_or(("",launch.parameters.as_str()))
    }else{
        let split=launch.parameters.find(char::is_whitespace).unwrap_or(launch.parameters.len());
        launch.parameters.split_at(split)
    };
    // A Python script is a known file argument. Leave arbitrary options and
    // other applications' argument strings alone.
    let python=Path::new(&repaired_exe.as_deref().unwrap_or(&launch.exe)).file_stem()
        .and_then(|stem|stem.to_str()).is_some_and(|stem|matches!(stem.to_ascii_lowercase().as_str(),"python"|"pythonw"));
    let corrected_script=(python&&[".py",".pyw"].iter().any(|ext|script.to_ascii_lowercase().ends_with(ext)))
        .then(||corrected_path(script,exists)).flatten();
    if repaired_exe.is_none()&&corrected_script.is_none(){return None;}
    let exe=quote(repaired_exe.as_deref().unwrap_or(&launch.exe));
    let params=corrected_script.map_or_else(||launch.parameters.clone(),|value|format!("{}{tail}",quote(&value)));
    Some(if params.is_empty(){exe}else{format!("{exe} {params}")})
}
pub(super) fn encode(line:&str,label:&str)->Result<String,PinError>{
    let Some(launch)=parse(line,label)else{return Err(PinError{message:"Nhập đường dẫn đầy đủ đến tệp .exe, rồi thêm tham số nếu cần.",suggestion:None});};
    let exists=|path:&str|Path::new(path).is_file();
    if let Some(suggestion)=suggest_repair(&launch,&exists){
        return Err(PinError{message:"Đường dẫn có dấu gạch thừa. Kiểm tra gợi ý rồi bấm Ghim lệnh lần nữa.",suggestion:Some(suggestion)});
    }
    if !absolute_exe(&launch.exe)||!exists(&launch.exe){
        return Err(PinError{message:"Không tìm thấy tệp .exe tại đường dẫn đã nhập.",suggestion:None});
    }
    Ok(key(&launch))
}
pub(crate) fn encode_application(path: &Path, label: &str, args: &str, directory: &str) -> Result<String, &'static str> {
    let exe = path.to_str().ok_or("Đường dẫn ứng dụng không hợp lệ.")?;
    if !absolute_exe(exe) || !path.is_file() {
        return Err("Chỉ EXE hỗ trợ đối số và thư mục làm việc.");
    }
    if args.len() > 4096 || [exe, label, args, directory].iter().any(|s| s.chars().any(char::is_control)) {
        return Err("Đối số hoặc thư mục làm việc không hợp lệ.");
    }
    let directory = if directory.trim().is_empty() {
        path.parent().and_then(Path::to_str).ok_or("Không xác định được thư mục làm việc.")?
    } else { directory.trim() };
    if !Path::new(directory).is_absolute() || !Path::new(directory).is_dir() {
        return Err("Chọn thư mục làm việc đầy đủ đang tồn tại.");
    }
    Ok(key(&Launch {exe:exe.into(), parameters:args.trim().into(), directory:directory.into(), label:label.into()}))
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
pub(crate) fn decode(value:&str)->Option<Launch>{
    let bytes=value.strip_prefix(PREFIX)?.as_bytes();
    if bytes.len()%2!=0||bytes.len()>30000{return None;}
    let digit=|b|match b{b'0'..=b'9'=>Some(b-b'0'),b'a'..=b'f'=>Some(b-b'a'+10),_=>None};
    let data=bytes.chunks_exact(2).map(|c|Some(digit(c[0])?*16+digit(c[1])?)).collect::<Option<Vec<_>>>()?;
    let [exe,parameters,directory,label]=serde_json::from_slice::<[String;4]>(&data).ok()?;
    (absolute_exe(&exe)&&Path::new(&directory).is_absolute()&&[&exe,&parameters,&directory,&label].into_iter()
        .all(|s|!s.chars().any(char::is_control))).then_some(Launch{exe,parameters,directory,label})
}
pub(crate) unsafe fn open(value:&str)->isize{
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
    fn offers_verified_markdown_escape_correction_without_changing_options(){
        let bad=parse(r#"E:\PandaTV\_Studio\venv\Scripts\pythonw\.exe "E:\PandaTV\_Studio\main.py" --mode literal\_arg"#,"PandaTV").unwrap();
        let good_exe=r"E:\PandaTV_Studio\venv\Scripts\pythonw.exe";
        let good_script=r"E:\PandaTV_Studio\main.py";
        let exists=|s:&str|s==good_exe||s==good_script;
        assert_eq!(suggest_repair(&bad,&exists),Some(format!(r"{good_exe} {good_script} --mode literal\_arg")));
        assert!(suggest_repair(&bad,&|_|false).is_none());
        let legitimate=parse(r"E:\Path\_tools\pythonw.exe E:\Path\_tools\main.py","Tools").unwrap();
        assert!(suggest_repair(&legitimate,&|_|true).is_none());
    }
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
