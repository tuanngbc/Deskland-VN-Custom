# Source verification runs only on a hosted runner; it does not change profiles.
$ErrorActionPreference='Stop'
if($env:GITHUB_ACTIONS -ne 'true'){throw 'Hosted verification only.'}
$literal='L"((?:\\.|[^"\\])*)"'
$table=[IO.File]::ReadAllText((Join-Path $PSScriptRoot '../native/fluidwin/traduzioni.h'))
$translated=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach($row in [regex]::Matches($table,'(?m)^\s*\{(.+)\},?\s*$')){
    $values=[regex]::Matches($row.Groups[1].Value,$literal)
    if($values.Count -ne 7 -or !$values[6].Groups[1].Value){throw 'Vietnamese translation row must preserve six upstream columns and a nonempty Vietnamese column.'}
    [void]$translated.Add($values[0].Groups[1].Value)
}
if($translated.Count -lt 300){throw 'Translation-table parser did not cover the native UI table.'}
$matchedCalls=0
$brands=@('TLauncher','FluidWin','Stage Manager','Stage Peek','FluidWin Launcher','Wi-Fi','Bluetooth','Windows Update','CPU','GPU','FPS')
$missing=[Collections.Generic.SortedSet[string]]::new([StringComparer]::Ordinal)
$files=@(Get-ChildItem (Join-Path $PSScriptRoot '../native/fluidwin'),(Join-Path $PSScriptRoot '../native/wallpaper') -File | Where-Object {$_.Extension -in '.h','.cpp' -and $_.Name -ne 'aggiorna.h'})
foreach($file in $files){
    $source=[IO.File]::ReadAllText($file.FullName)
    foreach($call in [regex]::Matches($source,'testi::t\(\s*'+$literal+'\s*,\s*'+$literal+'\s*\)')){
        $matchedCalls++
        $it=$call.Groups[1].Value;$en=$call.Groups[2].Value
        if(!$en -or $en -notmatch '[A-Za-z]' -or $en -in $brands -or $translated.Contains($en) -or $translated.Contains($it)){continue}
        [void]$missing.Add($en)
    }
}
[IO.Directory]::CreateDirectory('native-evidence')|Out-Null
@($missing)|Set-Content native-evidence/vietnamese-missing-static-keys.txt -Encoding UTF8
if($matchedCalls -lt 200){throw 'Localization audit did not cover the native UI call sites.'}
if($missing.Count){throw "Vietnamese static UI keys missing: $($missing.Count). See vietnamese-missing-static-keys.txt."}
'VIETNAMESE_STATIC_KEYS=PASS','UPSTREAM_COLUMNS_RETAINED=PASS','LAYOUT_AND_DYNAMIC_LABEL_AUDIT=STILL_REQUIRED'|Set-Content native-evidence/vietnamese-localization.txt
