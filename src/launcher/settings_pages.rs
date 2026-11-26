//! FluidWin v1.3.1 ricerca.h::pagine, in the existing Spotlight result owner.
//! Copyright (c) 2026 Clooudy, MIT; see third_party/fluidwin/LICENSE.
use super::{Entry,Path};
pub(super) const KIND:&str="Settings page";
pub(super) const OWN:&str="tlauncher:settings";
struct Page{vi:&'static str,en:&'static str,zh:&'static str,it:&'static str,keywords:&'static str,target:&'static str}
const PAGES:&[Page]=&[
    Page{vi:"Màn hình",en:"Display",zh:"显示",it:"Schermo",keywords:"display risoluzione monitor scala resolution scale do phan giai ty le man hinh 分辨率 缩放 显示器",target:"ms-settings:display"},
    Page{vi:"Âm thanh",en:"Sound",zh:"声音",it:"Audio",keywords:"suono volume microfono altoparlanti audio microphone speakers am luong micro loa 音量 麦克风 扬声器",target:"ms-settings:sound"},
    Page{vi:"Thông báo",en:"Notifications",zh:"通知",it:"Notifiche",keywords:"avvisi non disturbare alerts do not disturb khong lam phien 勿扰",target:"ms-settings:notifications"},
    Page{vi:"Bluetooth và thiết bị",en:"Bluetooth & devices",zh:"蓝牙和设备",it:"Bluetooth e dispositivi",keywords:"cuffie mouse tastiera headphones keyboard tai nghe chuot ban phim 耳机 鼠标 键盘",target:"ms-settings:bluetooth"},
    Page{vi:"Wi-Fi",en:"Wi-Fi",zh:"Wi-Fi",it:"Wi-Fi",keywords:"wireless rete internet network wifi mang khong day 无线 网络",target:"ms-settings:network-wifi"},
    Page{vi:"Mạng và Internet",en:"Network & internet",zh:"网络和 Internet",it:"Rete e Internet",keywords:"ethernet connessione proxy vpn connection ket noi 网络 连接",target:"ms-settings:network-status"},
    Page{vi:"Cá nhân hóa",en:"Personalization",zh:"个性化",it:"Personalizzazione",keywords:"aspetto appearance giao dien 外观",target:"ms-settings:personalization"},
    Page{vi:"Hình nền",en:"Background",zh:"背景",it:"Sfondo",keywords:"wallpaper desktop sfondo hinh nen 桌面 壁纸",target:"ms-settings:personalization-background"},
    Page{vi:"Màu sắc và chế độ tối",en:"Colors and dark mode",zh:"颜色和深色模式",it:"Colori e tema scuro",keywords:"modalità scura chiara colore dark light theme mau sac che do toi chu de 深色 浅色 主题",target:"ms-settings:colors"},
    Page{vi:"Ứng dụng đã cài",en:"Installed apps",zh:"已安装的应用",it:"App installate",keywords:"disinstalla programmi uninstall programs go cai dat chuong trinh 卸载 程序",target:"ms-settings:appsfeatures"},
    Page{vi:"Ứng dụng mặc định",en:"Default apps",zh:"默认应用",it:"App predefinite",keywords:"browser apri con open with trinh duyet mo bang 浏览器 打开方式",target:"ms-settings:defaultapps"},
    Page{vi:"Ứng dụng khởi động",en:"Startup apps",zh:"启动应用",it:"App all'avvio",keywords:"esecuzione automatica autostart khoi dong dang nhap 开机 自动启动",target:"ms-settings:startupapps"},
    Page{vi:"Tài khoản",en:"Accounts",zh:"帐户",it:"Account",keywords:"utente profilo user profile nguoi dung ho so 用户 个人资料",target:"ms-settings:yourinfo"},
    Page{vi:"Ngày và giờ",en:"Date & time",zh:"日期和时间",it:"Data e ora",keywords:"orologio fuso orario clock time zone dong ho mui gio 时钟 时区",target:"ms-settings:dateandtime"},
    Page{vi:"Ngôn ngữ và khu vực",en:"Language & region",zh:"语言和区域",it:"Lingua e area geografica",keywords:"italiano inglese formato english format tieng viet ngon ngu dinh dang 语言 格式",target:"ms-settings:regionlanguage"},
    Page{vi:"Bàn phím và nhập liệu",en:"Keyboard & typing",zh:"键盘和输入",it:"Tastiera e digitazione",keywords:"layout tastiera keyboard ban phim nhap lieu 布局 输入法",target:"ms-settings:typing"},
    Page{vi:"Chuột",en:"Mouse",zh:"鼠标",it:"Mouse",keywords:"puntatore velocità touchpad pointer speed con tro toc do ban di chuot 指针 速度 触摸板",target:"ms-settings:mousetouchpad"},
    Page{vi:"Nguồn điện",en:"Power",zh:"电源",it:"Energia",keywords:"batteria sospensione spegnimento battery sleep pin ngu tat may 电池 睡眠 关机",target:"ms-settings:powersleep"},
    Page{vi:"Lưu trữ",en:"Storage",zh:"存储",it:"Archiviazione",keywords:"spazio disco pulizia disk space cleanup dung luong o dia don dep 磁盘 空间 清理",target:"ms-settings:storagesense"},
    Page{vi:"Windows Update",en:"Windows Update",zh:"Windows 更新",it:"Windows Update",keywords:"aggiornamenti updates cap nhat 更新",target:"ms-settings:windowsupdate"},
    Page{vi:"Bảo mật Windows",en:"Windows Security",zh:"Windows 安全中心",it:"Sicurezza di Windows",keywords:"antivirus defender firewall chong virus tuong lua 防病毒 防火墙",target:"windowsdefender:"},
    Page{vi:"Quyền riêng tư và bảo mật",en:"Privacy & security",zh:"隐私和安全",it:"Privacy e sicurezza",keywords:"autorizzazioni fotocamera posizione permissions camera location quyen may anh vi tri 权限 相机 位置",target:"ms-settings:privacy"},
    Page{vi:"Máy in và máy quét",en:"Printers & scanners",zh:"打印机和扫描仪",it:"Stampanti e scanner",keywords:"stampa print may in may quet 打印 扫描",target:"ms-settings:printers"},
    Page{vi:"Thông tin hệ thống",en:"About",zh:"系统信息",it:"Informazioni sul sistema",keywords:"pc specifiche nome versione specs name version thong so ten phien ban 规格 名称 版本",target:"ms-settings:about"},
    Page{vi:"Trò chơi",en:"Gaming",zh:"游戏",it:"Gioco",keywords:"game bar modalità gioco game mode che do choi 游戏模式",target:"ms-settings:gaming-gamebar"},
    Page{vi:"Trợ năng",en:"Accessibility",zh:"辅助功能",it:"Accessibilità",keywords:"testo grande contrasto narratore large text contrast narrator chu lon tuong phan trinh doc man hinh 大文本 对比度 讲述人",target:"ms-settings:easeofaccess"},
    Page{vi:"Bảng điều khiển",en:"Control Panel",zh:"控制面板",it:"Pannello di controllo",keywords:"classico classic co dien 经典",target:"control.exe"},
    Page{vi:"Trình quản lý tác vụ",en:"Task Manager",zh:"任务管理器",it:"Gestione attività",keywords:"processi task manager processes tien trinh 进程",target:"Taskmgr.exe"},
    Page{vi:"Cài đặt TLauncher",en:"TLauncher settings",zh:"TLauncher 设置",it:"Impostazioni di FluidWin",keywords:"dock gelatina genio panoramica jelly genie overview cai dat 设置 程序坞",target:OWN},
];
fn fold(value:&str)->String{
    value.to_lowercase().chars().filter_map(|c|Some(match c{
        'à'|'á'|'ả'|'ã'|'ạ'|'ă'|'ằ'|'ắ'|'ẳ'|'ẵ'|'ặ'|'â'|'ầ'|'ấ'|'ẩ'|'ẫ'|'ậ'=>'a',
        'è'|'é'|'ẻ'|'ẽ'|'ẹ'|'ê'|'ề'|'ế'|'ể'|'ễ'|'ệ'=>'e','ì'|'í'|'ỉ'|'ĩ'|'ị'=>'i',
        'ò'|'ó'|'ỏ'|'õ'|'ọ'|'ô'|'ồ'|'ố'|'ổ'|'ỗ'|'ộ'|'ơ'|'ờ'|'ớ'|'ở'|'ỡ'|'ợ'=>'o',
        'ù'|'ú'|'ủ'|'ũ'|'ụ'|'ư'|'ừ'|'ứ'|'ử'|'ữ'|'ự'=>'u','ỳ'|'ý'|'ỷ'|'ỹ'|'ỵ'=>'y','đ'=>'d',
        '\u{0300}'..='\u{036f}'=>return None,_=>c,
    })).collect()
}
fn page(path:&Path)->Option<&'static Page>{
    let target=path.to_str()?;
    if !target.starts_with("ms-settings:")&&!matches!(target,"windowsdefender:"|OWN|"control.exe"|"Taskmgr.exe"){return None;}
    PAGES.iter().find(|p|p.target==target)
}
pub(super) fn is_entry(entry:&Entry)->bool{entry.kind==KIND&&page(&entry.path).is_some()}
pub(super) fn allowed(path:&Path)->bool{page(path).is_some()}
pub(super) fn entries()->Vec<Entry>{
    PAGES.iter().map(|p|Entry{name:super::localized(p.vi,p.en,p.zh).into(),path:p.target.into(),kind:KIND,
        detail:super::localized("Cài đặt","Settings","设置").into()}).collect()
}
pub(super) fn rank(entry:&Entry,query:&str)->Option<u8>{
    // Existing app/System rows retain their identity and history even when
    // they share a target with one of the original settings shortcuts.
    let page=page(&entry.path).or_else(||{
        let name=entry.path.file_name()?.to_str()?;
        if !name.eq_ignore_ascii_case("control.exe")&&!name.eq_ignore_ascii_case("taskmgr.exe"){return None;}
        let identity=super::catalog::identity(&entry.path);
        PAGES.iter().filter(|p|matches!(p.target,"control.exe"|"Taskmgr.exe")).find(|page|super::catalog::identity(Path::new(page.target))==identity)
    })?;let query=fold(query.trim());
    if query.is_empty()||query.len()>512||query.chars().any(char::is_control){return None;}
    let names=[page.vi,page.en,page.zh,page.it].map(fold);
    if names.iter().any(|name|name==&query){return Some(0);}
    if names.iter().any(|name|name.starts_with(&query)){return Some(1);}
    let keywords=fold(&format!("{} {} {} {} {}",page.vi,page.en,page.zh,page.it,page.keywords));
    query.split_whitespace().all(|word|keywords.contains(word)).then_some(2)
}
pub(super) fn extend(rows:&mut Vec<Entry>){
    let mut seen=rows.iter().map(|e|super::catalog::identity(&e.path)).collect::<std::collections::HashSet<_>>();
    rows.extend(entries().into_iter().filter(|e|seen.insert(super::catalog::identity(&e.path))));
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn original_29_pages_keep_exact_allowlisted_targets_and_multilingual_keywords(){
        assert_eq!(PAGES.len(),29);let entries=entries();assert_eq!(entries.len(),29);
        for entry in &entries{assert!(allowed(&entry.path));assert!(is_entry(entry));}
        for (query,target) in [("độ phân giải","ms-settings:display"),("do phan giai","ms-settings:display"),("分辨率","ms-settings:display"),
            ("microphone","ms-settings:sound"),("ngôn ngữ","ms-settings:regionlanguage"),("uninstall programs","ms-settings:appsfeatures"),
            ("勿扰","ms-settings:notifications"),("gelatina",OWN),("processi","Taskmgr.exe")]{
            assert!(entries.iter().any(|e|e.path==Path::new(target)&&rank(e,query).is_some()),"missing original/localized query: {query}");
        }
        for bad in ["ms-settings:unknown","ms-settings:display?x=1","windowsdefender:evil","tlauncher:settings/extra","control.exe /evil","*fluidwin"]{assert!(!allowed(Path::new(bad)));}
        assert!(entries.iter().all(|e|rank(e,"").is_none()&&rank(e,"\n").is_none()));
        let mut rows=vec![entries[0].clone()];extend(&mut rows);assert_eq!(rows.len(),29);
    }
}
