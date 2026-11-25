# Prepare an immutable owner candidate using a GitHub Actions package. Never run it.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][long]$RunId,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-f]{40}$')][string]$Commit,
    [string]$ProductionExe = 'E:\TLauncher\TLauncher.exe'
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$version='1.0.3'
$repo='tuanngbc/TLauncher'
$api="https://api.github.com/repos/$repo"
$ProductionExe=[IO.Path]::GetFullPath($ProductionExe)
if ((Get-Item -LiteralPath $ProductionExe).VersionInfo.ProductVersion -cne '1.0.2') { throw 'Expected the verified v1.0.2 production installation.' }
$root=[IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'TLauncher-UAT'))
$desktop=[Environment]::GetFolderPath('DesktopDirectory')
if (!$desktop -or !(Test-Path -LiteralPath $desktop -PathType Container)) { throw 'The Windows Desktop directory is unavailable.' }

function Assert-NoReparse([string]$Path) {
    $p=[IO.Path]::GetFullPath($Path)
    while ($p) {
        if ((Test-Path -LiteralPath $p) -and ((Get-Item -LiteralPath $p -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Unsafe link or junction at $p"
        }
        $parent=[IO.Path]::GetDirectoryName($p)
        if (!$parent -or $parent -eq $p) { break }
        $p=$parent
    }
}

Assert-NoReparse $root
Assert-NoReparse $ProductionExe
$headers=@{Accept='application/vnd.github+json';'X-GitHub-Api-Version'='2022-11-28'}
$run=Invoke-RestMethod -Uri "$api/actions/runs/$RunId" -Headers $headers
if ($run.head_sha -cne $Commit -or $run.head_branch -cne 'codex/fluidwin-integration-v1.0.3' -or
    $run.path -cne '.github/workflows/build-rust-test.yml' -or $run.status -ne 'completed' -or $run.conclusion -ne 'success') {
    throw 'The exact development-branch workflow has not passed.'
}
$items=Invoke-RestMethod -Uri "$api/actions/runs/$RunId/artifacts" -Headers $headers
$artifact=@($items.artifacts | Where-Object {$_.name -ceq 'TLauncher-Rust-win-x64' -and !$_.expired})
if ($artifact.Count -ne 1 -or $artifact[0].workflow_run.head_sha -cne $Commit) { throw 'Candidate artifact is missing or ambiguous.' }
$check=$artifact[0].digest
if ($check -cnotmatch '^sha256:[a-f0-9]{64}$') { throw 'Missing GitHub archive checksum.' }

$staging=Join-Path $root ('downloads-'+$Commit.Substring(0,12))
$archive=Join-Path $staging 'actions.zip'
Assert-NoReparse $archive
New-Item -ItemType Directory -Path $staging -Force | Out-Null
if (!(Test-Path -LiteralPath $archive) -or ('sha256:'+(Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant()) -cne $check) {
    $credentials=@('protocol=https','host=github.com','') | git credential fill 2>$null
    $secret=$credentials | Where-Object {$_ -like 'password=*'} | Select-Object -First 1
    if (!$secret) { throw 'GitHub artifact download requires the repository credential.' }
    try {
        $headers.Authorization='Bearer '+$secret.Substring(9)
        $uri="$api/actions/artifacts/$($artifact[0].id)/zip"
        Invoke-WebRequest -Uri $uri -Headers $headers -OutFile $archive
    } finally {
        $headers.Remove('Authorization')
        Remove-Variable credentials,secret -ErrorAction SilentlyContinue
    }
}
if (('sha256:'+(Get-FileHash -LiteralPath $archive).Hash.ToLowerInvariant()) -cne $check) { throw 'GitHub artifact checksum mismatch.' }

Add-Type -AssemblyName System.IO.Compression.FileSystem
$zipName="TLauncher-Rust-$($Commit.Substring(0,7))-win-x64"
$zip=Join-Path $staging "$zipName.zip"
Assert-NoReparse $zip
$container=[IO.Compression.ZipFile]::OpenRead($archive)
try {
    $image=$container.GetEntry("$zipName.zip")
    $checksum=$container.GetEntry("$zipName.zip.sha256")
    if (!$image -or !$checksum -or $checksum.Length -gt 256 -or $image.Length -gt 150MB) { throw 'Unexpected artifact contents.' }
    $reader=[IO.StreamReader]::new($checksum.Open())
    try { $want=($reader.ReadToEnd().Trim() -split '\s+')[0] }finally{$reader.Dispose()}
    if ($want -cnotmatch '^[a-f0-9]{64}$') { throw 'Invalid package checksum.' }
    if (!(Test-Path -LiteralPath $zip) -or (Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant() -cne $want) {
        [IO.Compression.ZipFileExtensions]::ExtractToFile($image,$zip,$true)
    }
} finally { $container.Dispose() }
if ((Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant() -cne $want) { throw 'Candidate ZIP checksum mismatch.' }

$target=Join-Path $root ('candidate-'+$version+'-'+$Commit.Substring(0,12))
$profile=Join-Path $root 'profiles\tuanpc-desk-trial-owner-1-0-3'
$exe=Join-Path $target 'TLauncher.exe'
$manifest=Join-Path $target 'verified-artifact.json'
$launcher=Join-Path $target 'owner-uat.ps1'
$linkPath=Join-Path $desktop ("TLauncher $version "+[char]0x2014+' TEST.lnk')
foreach ($file in $target,$profile,$exe,$manifest,$launcher,$linkPath) { Assert-NoReparse $file }

$lock=[Threading.Mutex]::new($false,'Local\TLauncherOwnerUatSwitch')
$locked=$false
try {
    try { $locked=$lock.WaitOne(0) } catch [Threading.AbandonedMutexException] { $locked=$true }
    if (!$locked) { throw 'An owner trial is active. This candidate and shortcut were not changed.' }
    $session=(Get-Process -Id $PID).SessionId
    $processes=@(Get-CimInstance Win32_Process -Filter "Name='TLauncher.exe'" | Where-Object SessionId -eq $session)
    if (@($processes | Where-Object {!$_.ExecutablePath -or !([IO.Path]::GetFullPath($_.ExecutablePath).Equals($ProductionExe,[StringComparison]::OrdinalIgnoreCase))}).Count) {
        throw 'A running launcher session is not the verified production executable. Candidate handoff was left unchanged.'
    }

    $exists=Test-Path -LiteralPath $target
    if ($exists -and !(Test-Path -LiteralPath $manifest -PathType Leaf)) {
        throw 'An incomplete candidate directory already exists; it was left untouched.'
    }
    if (!$exists) { New-Item -ItemType Directory -Path $target | Out-Null }
    $contained=[IO.Compression.ZipFile]::OpenRead($zip)
    try {
        $claimed=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $unpacked=0L
        $members=@(foreach($entry in $contained.Entries){
            $name=$entry.FullName.Replace('\','/')
            if ($name -ceq "$zipName/") { continue }
            if (!$name.StartsWith("$zipName/",[StringComparison]::Ordinal)) { throw 'Unexpected candidate ZIP root.' }
            $relative=$name.Substring($zipName.Length+1).TrimEnd('/')
            if (!$relative -or $relative.Split('/') -match '^(?:\.|\.\.)$|[:<>|?*"]|[. ]$') { throw 'Invalid candidate ZIP path.' }
            $destination=[IO.Path]::GetFullPath((Join-Path $target $relative))
            if (!$destination.StartsWith($target+'\',[StringComparison]::OrdinalIgnoreCase) -or
                !$claimed.Add($destination) -or (($entry.ExternalAttributes -shr 16) -band 0xF000) -eq 0xA000) { throw 'Duplicate, linked or uncontained candidate ZIP entry.' }
            if ($name.EndsWith('/')) { continue }
            if ($relative -ieq 'verified-artifact.json') { throw 'Package must not provide a trusted owner manifest.' }
            $unpacked += $entry.Length
            if ($unpacked -gt 350MB -or $claimed.Count -gt 5000) { throw 'Candidate ZIP exceeds unpacking budget.' }
            [pscustomobject]@{ Entry=$entry; Location=$destination }
        })
        foreach ($member in $members) {
            Assert-NoReparse $member.Location
            if (!$exists) {
                [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($member.Location)) | Out-Null
                [IO.Compression.ZipFileExtensions]::ExtractToFile($member.Entry,$member.Location,$false)
            } else {
                $memberStream=$member.Entry.Open()
                try {
                    $sha=[Security.Cryptography.SHA256]::Create()
                    try { $expected=[BitConverter]::ToString($sha.ComputeHash($memberStream)).Replace('-','') }
                    finally { $sha.Dispose() }
                } finally { $memberStream.Dispose() }
                if ((Get-FileHash -LiteralPath $member.Location).Hash -cne $expected) { throw 'Existing candidate differs from the CI archive.' }
            }
        }
    } finally { $contained.Dispose() }
    if ((Get-Content -LiteralPath (Join-Path $target 'BUILD-COMMIT.txt') -Raw).Trim() -cne $Commit -or
        (Get-Content -LiteralPath (Join-Path $target 'VERSION.txt') -Raw).Trim() -cne $version -or
        (Get-Item -LiteralPath $exe).VersionInfo.ProductVersion -cne $version -or
          !(Test-Path -LiteralPath $launcher -PathType Leaf)) { throw 'Candidate version, source commit, or launcher is wrong.' }
    $binary=[IO.BinaryReader]::new([IO.File]::OpenRead($exe))
    try {
        if ($binary.ReadUInt16() -ne 0x5a4d) { throw 'Invalid Windows executable header.' }
        $binary.BaseStream.Position=0x3c
        $binary.BaseStream.Position=$binary.ReadUInt32()
        if ($binary.ReadUInt32() -ne 0x4550 -or $binary.ReadUInt16() -ne 0x8664) { throw 'Expected a Windows x64 executable.' }
    } finally { $binary.Dispose() }
    $expectedExe=(Get-FileHash -LiteralPath $exe).Hash.ToLowerInvariant()
    $manifestData=@{sourceCommit=$Commit;runId=$RunId;artifactId=$artifact[0].id;artifactSha256=$check.Substring(7);
        archiveSha256=$want;exeSha256=$expectedExe;profile=$profile;productionExe=$ProductionExe;version=$version}
    if ($exists) {
        $previous=Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
        if ($previous.sourceCommit -cne $Commit -or $previous.runId -ne $RunId -or $previous.exeSha256 -cne $expectedExe -or
            $previous.profile -ine $profile -or $previous.productionExe -ine $ProductionExe) { throw 'Existing candidate manifest differs.' }
    } else {
        $manifestData | ConvertTo-Json | Set-Content -LiteralPath $manifest -Encoding UTF8
    }
    $profileMarker=Join-Path $profile '.tuanpc-desk-trial'
    Assert-NoReparse $profileMarker
    $markerText="tuanpc-desk isolated native smoke profile v1`n"
    if (Test-Path -LiteralPath $profile) {
        if (!(Test-Path -LiteralPath $profileMarker -PathType Leaf) -or [IO.File]::ReadAllText($profileMarker) -cne $markerText) {
            throw 'Existing owner trial profile does not have a valid isolation marker.'
        }
    } else {
        New-Item -ItemType Directory -Path $profile -Force | Out-Null
        [IO.File]::WriteAllText($profileMarker,$markerText,[Text.UTF8Encoding]::new($false))
    }
    $command=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $wsh=New-Object -ComObject WScript.Shell
    $shortcut=$wsh.CreateShortcut($linkPath)
    $shortcut.TargetPath=$command
    $shortcut.Arguments='-NoProfile -STA -ExecutionPolicy Bypass -File "'+$launcher+'" -UseVerifiedManifest'
    $shortcut.WorkingDirectory=$target
    $shortcut.IconLocation=$exe+',0'
    $shortcut.Description="TLauncher $version owner trial. Exit the app to restore the previous launcher."
    $shortcut.Save()
    $readback=$wsh.CreateShortcut($linkPath)
    if ($readback.TargetPath -ine $command -or $readback.Arguments -cne $shortcut.Arguments -or $readback.WorkingDirectory -ine $target) {
        throw 'Desktop shortcut target could not be verified.'
    }
    [pscustomobject]@{ DesktopShortcut=$linkPath; Candidate=$target; SourceCommit=$Commit; Workflow=$run.html_url; PackageSHA256=$want; Version=$version }
} finally {
    if ($locked) { $lock.ReleaseMutex() }
    $lock.Dispose()
}
