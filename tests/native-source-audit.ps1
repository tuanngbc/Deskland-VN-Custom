# Run exclusively on a GitHub-hosted runner.
$ErrorActionPreference='Stop'
$app=Get-Content native/fluidwin/fluidwin.cpp -Raw
if($app -match '#include\s+"aggiorna.h"|aggiorna::|gAgg|WM_AGGIORNA') {throw 'Updater execution path remains'}
$settings=Get-Content native/fluidwin/impostazioni.h -Raw
if($settings -match 'Valori::aggiornamenti|v\.aggiornamenti') {throw 'Updater Settings path remains'}
$files=@(Get-ChildItem native/fluidwin -File | Where-Object {$_.Extension -in '.h','.cpp' -and $_.Name -ne 'aggiorna.h'})
foreach($file in $files) {
    $s=Get-Content $file.FullName -Raw
    if($s -match 'L"Software\\\\FluidWin|L"Local\\\\FluidWin\.|L"FluidWin\.') {throw "Upstream runtime identity leaked in $($file.Name)"}
}
$p=Get-Content native/fluidwin/PROVENANCE.json -Raw|ConvertFrom-Json
if($p.commit -cne '2f773a8733b84c7dbee88fff6628d14e9ac802b9'){throw 'Wrong upstream'}
foreach($file in $p.files){if(!(Test-Path (Join-Path 'native/fluidwin' $file.path))){throw 'Incomplete native source import'}}
if((Get-FileHash assets/app.ico).Hash -cne (Get-FileHash native/fluidwin/icona.ico).Hash){throw 'Wrong product icon'}
New-Item -ItemType Directory -Path native-evidence -Force | Out-Null
'UPDATER_EXECUTION=ABSENT','NAMESPACE_AUDIT=PASS','COMPLETE_APPLICATION_IMPORT=PASS','TLAUNCHER_ICON=PASS' | Set-Content native-evidence/source-audit.txt
