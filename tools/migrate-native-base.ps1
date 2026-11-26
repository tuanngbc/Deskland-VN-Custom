# Source-only, reproducible migration from the pinned checkout. Never builds/runs an app.
param([string]$Reference='E:\FluidWin-reference-v1.3.1')
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$upstream='2f773a8733b84c7dbee88fff6628d14e9ac802b9'
if ((git -C $Reference rev-parse HEAD) -cne $upstream -or (git -C $Reference status --porcelain)) {throw 'Expected untouched pinned FluidWin checkout.'}
$dest=Join-Path $root 'native\fluidwin'
if(Test-Path -LiteralPath $dest){throw 'Migration destination exists; do not overwrite working source.'}
New-Item -ItemType Directory -Path $dest | Out-Null
$provenance=@()
foreach($relative in (git -C $Reference ls-files)) {
    # Include the complete application sources, embedded assets and build recipe.
    # Upstream showcase recordings are documentation, not runtime dependencies.
    if($relative.StartsWith('docs/')){continue}
    $source=Join-Path $Reference $relative
    $target=Join-Path $dest $relative
    New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination $target
    $provenance += [ordered]@{path=$relative;upstreamSha256=(Get-FileHash -LiteralPath $source).Hash.ToLowerInvariant()}
}
function Edit([string]$File,[scriptblock]$Change) {
    $path=Join-Path $dest $File
    $text=[IO.File]::ReadAllText($path).Replace("`r`n","`n")
    $updated=& $Change $text
    [IO.File]::WriteAllText($path,$updated,[Text.UTF8Encoding]::new($false))
}
# Identity categories are deliberately distinct: permanent window/IPC identifiers,
# profile keys, product text, startup identity and user asset paths. No identifier
# or algorithm names are renamed; attribution/URLs/comments retain their origin.
$identityChanges=@()
foreach($file in Get-ChildItem -LiteralPath $dest -File | Where-Object {$_.Extension -in '.h','.cpp'}) {
    if($file.Name -eq 'aggiorna.h'){continue} # preserved source, not compiled/packaged
    $name=$file.Name
    Edit $name {
        param($s)
        if($s.Contains('#pragma once')){$s=$s.Replace('#pragma once','#pragma once'+"`n"+'#include "tlauncher_identity.h"')}
        $s=[regex]::Replace($s,'L"(?:[^"\\]|\\.)*"',{
            param($m)
            $literal=$m.Value
            $value=$literal.Substring(2,$literal.Length-3)
            $new=$literal
            if($value.StartsWith('Software\\FluidWin')) {
                $suffix=$value.Substring('Software\\FluidWin'.Length)
                switch -CaseSensitive ($suffix) {
                    '' {$new='tlauncher::registry()'}
                    '\\Impostazioni' {$new='tlauncher::settingsRegistry()'}
                    '\\Banner' {$new='tlauncher::bannerRegistry()'}
                    default {throw "Unaudited registry suffix $suffix"}
                }
            } elseif($value.StartsWith('FluidWin.')) {
                $new='L"TLauncher.Native.'+$value.Substring(9)+'"'
            } elseif($value.StartsWith('Local\\FluidWin.')) {
                $new='L"Local\\TLauncher.Native.'+$value.Substring('Local\\FluidWin.'.Length)+'"'
            } elseif($value -eq '*fluidwin') {$new='L"*tlauncher-native"'}
            elseif($value -eq 'FluidWin' -and $name -eq 'impostazioni.h') {$new='L"TLauncher"'}
            elseif($value -eq '%LOCALAPPDATA%\\FluidWin\\Cursori') {$new='L"%TLAUNCHER_NATIVE_TRIAL_DIR%\\Cursori"'}
            elseif($value -eq '\\FluidWin') {$new='L"\\TLauncher-Native"'}
            elseif($value.StartsWith('FluidWin-')) {$new='L"TLauncher-Native-'+$value.Substring(9)+'"'}
            elseif($value -eq 'FluidWin-diagnostica.txt') {$new='L"TLauncher-Native-diagnostica.txt"'}
            elseif($value -eq 'fluidwin.log') {$new='L"tlauncher-native.log"'}
            elseif($value.Contains('FluidWin') -and !$value.Contains('github.com/clooudfn/FluidWin') -and !$value.Contains('/repos/')) {
                # User-visible product strings and their translation keys only.
                $new='L"'+$value.Replace('FluidWin','TLauncher')+'"'
            }
            if($new -cne $literal){$script:identityChanges += [ordered]@{file=$name;from=$literal;to=$new}}
            return $new
        })
        $s=$s.Replace('L"TLauncher.Native.", 9','L"TLauncher.Native.", 17')
        return $s
    }
}
Edit 'fluidwin.cpp' {
    param($s)
    $s=$s.Replace('#include "aggiorna.h"','// Auto Update excluded by owner: no updater translation unit or call sites.')
    $s=$s.Replace('#define WIN32_LEAN_AND_MEAN','#include "tlauncher_identity.h"'+"`n"+'#define WIN32_LEAN_AND_MEAN')
    $start=$s.IndexOf('// ---- aggiornamenti (aggiorna.h):')
    $end=$s.IndexOf('static LRESULT CALLBACK procMessaggi',$start)
    if($start -lt 0 -or $end -lt 0){throw 'Updater block not found'}
    $s=$s.Remove($start,$end-$start)
    $s=[regex]::Replace($s,'(?m)^.*WM_AGGIORNA.*\n','')
    $s=[regex]::Replace($s,'(?m)^.*aggiorna::recupera.*\n','')
    $s=[regex]::Replace($s,'(?m)^.*if \(w == (21|22|23)\).*\n','')
    $start=$s.IndexOf('    // aggiornamenti: via il FluidWin.exe')
    $end=$s.IndexOf('    if (gDockOk) {',$start)
    if($start -lt 0 -or $end -lt 0){throw 'Updater startup block not found'}
    $s=$s.Remove($start,$end-$start)
    $s=[regex]::Replace($s,'(?m)^.*std::wstring versioneFinta;.*\n','')
    $s=[regex]::Replace($s,'(?m)^.*if \(!wcscmp\(argv\[i\], L"--versione-finta"\).*\n','')
    $s=$s.Replace('    aggiorna::ferma();','')
    $start=$s.IndexOf('    if (gAggThread &&')
    $end=$s.IndexOf('    if (gLog) fclose(gLog);',$start)
    if($start -lt 0 -or $end -lt 0){throw 'Updater shutdown block not found'}
    $s=$s.Remove($start,$end-$start)
    $s=$s.Replace('0x464C5753','0x544C4E53').Replace('0x464C5744','0x544C4E44')
    $s=$s.Replace('int WINAPI wWinMain(HINSTANCE inst, HINSTANCE, PWSTR, int) {',@'
int WINAPI wWinMain(HINSTANCE inst, HINSTANCE, PWSTR, int) {
    // Validate the isolated owner profile before recovery, COM, hooks or GPU.
    if (!tlauncher::validateTrial()) return 2;
    if (wcsstr(GetCommandLineW(), L"--quit")) {
        if (HWND target=FindWindowW(L"TLauncher.Native.Messaggi", nullptr)) PostMessageW(target, WM_CLOSE, 0, 0);
        return 0;
    }
'@)
    $s=$s.Replace('gDock.impostaNotch(v.notch);','gDock.impostaNotch(v.notch); gDock.impostaNotchEnabled(v.notchEnabled);')
    # Upstream renderer and original durations/easing remain unchanged in Phase 1.
    return $s
}
Edit 'impostazioni.h' {
    param($s)
    $s=$s.Replace('#pragma once','#pragma once'+"`n"+'#include "tlauncher_identity.h"')
    $s=[regex]::Replace($s,'(?m)^.*bool aggiornamenti =.*\n','')
    $s=[regex]::Replace($s,'(?m)^.*v\.aggiornamenti =.*\n','')
    $s=[regex]::Replace($s,' scriviDword\(L"Aggiornamenti", v\.aggiornamenti\);','')
    $s=[regex]::Replace($s,'(?m)^        \{L"Cerca aggiornamenti"[^\n]*\n[^\n]*&Valori::aggiornamenti[^\n]*\n','')
    $s=$s.Replace('inline std::wstring comandoRegistrato() {','inline std::wstring comandoRegistrato() {'+"`n"+'    if (tlauncher::trial()) return L"";')
    $s=$s.Replace('inline void impostaAvvio(bool si) {','inline void impostaAvvio(bool si) {'+"`n"+'    if (tlauncher::trial()) return; // Never alter production startup from UAT.')
    $s=$s.Replace('    bool notch = true;','    bool notchEnabled = true;    // True lifetime switch; notch below selects always/activity mode.'+"`n"+'    bool notch = true;')
    $s=$s.Replace('    v.notch = leggiDword', '    v.notchEnabled = leggiDword(L"NotchEnabled", 1);'+"`n"+'    v.notch = leggiDword')
    $s=$s.Replace('    scriviDword(L"Notch",','    scriviDword(L"NotchEnabled", v.notchEnabled);'+"`n"+'    scriviDword(L"Notch",')
    $s=$s.Replace('        {L"Notch", L"Notch",',@'
        {L"Dynamic Notch", L"Dynamic Notch",
         L"Abilita o disabilita completamente", L"Enable or disable completely", &Valori::notchEnabled, S_NOTCH},
        {L"Sempre visibile", L"Show always",
'@)
    $s=$s.Replace('L"Always visible. Off: it appears only when needed"','L"Off: show only when active"')
    $s=$s.Replace('L"Notch & notifications"','L"Dynamic Notch"')
    $s=$s.Replace('testi::t(L"Windows reimmaginato · github.com/clooudfn/FluidWin", L"Windows reimagined · github.com/clooudfn/FluidWin")','L"Your Windows. Your Workspace."')
    $s=$s.Replace('            // diagnostica: versione, Windows',@'
            riga(L"FluidWin 1.3.1 · Copyright (c) 2026 Clooudy", L"MIT · github.com/clooudfn/FluidWin · native desktop foundation");
            riga(L"TLauncher · GPL-3.0", L"tuanngbc/TLauncher · BLEZZ0102/Deskland · nhhoang188/Deskland-VN");
            // diagnostica: versione, Windows
'@)
    return $s
}
Edit 'notch.h' {
    param($s)
    $s=$s.Replace('    bool crea(HINSTANCE inst) {','    bool crea(HINSTANCE inst) {'+"`n"+'        instance = inst; if (!enabled || fin) return fin != nullptr;')
    $s=$s.Replace('    ~Notch() {',@'
    // Independent lifetime switch. Upstream abilita() still selects fixed/activity.
    void enable(bool si) {
        if (si == enabled) return;
        enabled = si;
        if (!si) {
            if (fin) { KillTimer(fin, 2); vetro::fermaBattito(fin, 1); }
            visibile=false; animando=false; sopra=false; attesa=0;
            finisciNotifica(); fineVolume=0; fineAvviso=0;
            if (fin) { RevokeDragDrop(fin); SetWindowLongPtrW(fin, 0, 0); DestroyWindow(fin); fin=nullptr; }
            svuotaScaffale(); libera(); W2=H2=0;
            for (auto** f : {&fOra,&fGiorno,&fTitolo,&fPiccolo,&fTesto,&fNumero,&fGrande}) if (*f) {(*f)->Release();*f=nullptr;}
            fDpi=0;
            // Reader is also the Dock media source. Keep it for that shared
            // consumer while Dock is active; never keep a Notch-only worker.
        } else {
            crea(instance); superficie(); misuraOra(); bersagli(true); visibilita();
        }
    }
    bool isEnabled() const { return enabled; }
    ~Notch() {
'@)
    $s=$s.Replace('        if (si) lettore.avvia();','        if (si) lettore.avvia(); else lettore.ferma();')
    $s=$s.Replace('    HWND fin = nullptr;','    HINSTANCE instance = nullptr;'+"`n"+'    bool enabled = true;'+"`n"+'    HWND fin = nullptr;')
    $s=$s.Replace('        superficie(); misuraOra(); bersagli(true); ridisegna();','        if (enabled && fin) { superficie(); misuraOra(); bersagli(true); ridisegna(); }')
    $s=$s.Replace('        superficie();         //','        if (!enabled) return;'+"`n"+'        superficie();         //')
    $s=$s.Replace('        const bool si = acceso && modoAtt != NASCOSTO && fin;','        const bool si = enabled && acceso && modoAtt != NASCOSTO && fin;')
    foreach($signature in @('    void anima() {','    void misuraOra() {','    void ridisegna() {','    void bersagli(bool subito = false) {')) {
        $s=$s.Replace($signature,$signature+"`n"+'        if (!enabled || !fin) return;')
    }
    $s=$s.Replace('const musica::Brano& musicaOra() const { return brano; }','const musica::Brano& musicaOra() { if (!enabled) brano=lettore.stato(); return brano; }')
    $s=$s.Replace('    void superficie() {','    void superficie() {'+"`n"+'        if (!enabled || !fin) return;')
    $s=$s.Replace('        if (!visibile) return;'+"`n"+'        const double t = adesso();', '        if (!enabled || !visibile) return;'+"`n"+'        const double t = adesso();')
    return $s
}
Edit 'dock.h' {param($s) $s=$s.Replace('    void impostaNotch(bool si)', '    void impostaNotchEnabled(bool si) { notch.enable(si); dirty = true; }'+"`n"+'    void impostaNotch(bool si)'); return $s}
Copy-Item -LiteralPath (Join-Path $root 'assets\app.ico') -Destination (Join-Path $dest 'icona.ico') -Force
Edit 'icona.rc' {param($s)
    $s=$s.Replace('1,3,1,0','1,0,3,0').Replace('"1.3.1"','"1.0.3"')
    $s=$s.Replace('"Clooudy"','"TLauncher contributors"').Replace('"FluidWin.exe"','"TLauncher.exe"').Replace('"FluidWin"','"TLauncher"')
    $s=$s.Replace('Copyright (c) 2026 Clooudy - licenza MIT','TLauncher GPL-3.0; FluidWin Copyright (c) 2026 Clooudy - MIT')
    return $s
}
[ordered]@{repository='https://github.com/clooudfn/FluidWin';version='1.3.1';commit=$upstream;license='MIT';copyright='Copyright (c) 2026 Clooudy';files=$provenance;identityChanges=$identityChanges} |
    ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $dest 'PROVENANCE.json') -Encoding UTF8
