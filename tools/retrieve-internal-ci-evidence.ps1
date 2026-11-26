param(
    [Parameter(Mandatory=$true)][long]$RunId,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$ExpectedCommit,
    [Parameter(Mandatory=$true)][string]$EvidenceRoot
)
$ErrorActionPreference='Stop'
# Download/extract verified hosted evidence only. Never executes a candidate,
# creates a trial profile, changes a shortcut, or switches production.
$api='https://api.github.com/repos/tuanngbc/TLauncher'
$run=Invoke-RestMethod "$api/actions/runs/$RunId"
if ($run.conclusion -ne 'success' -or $run.head_sha -ne $ExpectedCommit -or
    $run.path -ne '.github/workflows/build-rust-test.yml' -or $run.event -notin @('push','workflow_dispatch')) {
    throw 'The hosted workflow/source does not match the requested successful internal milestone.'
}
$listing=Invoke-RestMethod $run.artifacts_url
$selected=@($listing.artifacts | Where-Object {$_.name -in @('TLauncher-Rust-win-x64','native-visual-fixtures')})
if ($selected.Count -ne 2 -or @($selected | Where-Object expired).Count) {throw 'Required hosted evidence has expired or is missing.'}
$credentialFields=("protocol=https`nhost=github.com`n`n" | git credential fill)
$credentialValue=$credentialFields | Where-Object {$_.StartsWith('password=')}
if (-not $credentialValue) {throw 'GitHub artifact access is unavailable.'}
$headers=@{Authorization=('Bearer '+$credentialValue.Substring(9))}
$destination=Join-Path $EvidenceRoot ('internal-'+$ExpectedCommit.Substring(0,7))
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$archives=@()
foreach ($artifact in $selected) {
    $zip=Join-Path $destination ($artifact.name+'.zip')
    $expectedDigest=[string]$artifact.digest
    if ($expectedDigest -notmatch '^sha256:[0-9a-f]{64}$') {throw 'GitHub did not supply a usable artifact checksum.'}
    $existing=Test-Path -LiteralPath $zip -PathType Leaf
    if (-not $existing -or ('sha256:'+(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLower()) -ne $expectedDigest) {
        Invoke-WebRequest -Uri $artifact.archive_download_url -Headers $headers -OutFile $zip
    }
    $hash=(Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLower()
    if ('sha256:'+$hash -ne $expectedDigest) {throw 'GitHub archive checksum mismatch.'}
    Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $destination $artifact.name) -Force
    $archives+=@{name=$artifact.name;id=$artifact.id;sha256=$hash}
}
$dist=Join-Path $destination 'TLauncher-Rust-win-x64\dist'
$packages=@(Get-ChildItem -LiteralPath $dist -File -Filter '*.zip')
if ($packages.Count -ne 1) {throw 'Ambiguous internal package.'}
$package=$packages[0]
$packageHash=(Get-FileHash -LiteralPath $package.FullName -Algorithm SHA256).Hash.ToLower()
$sidecar=(Get-Content -LiteralPath ($package.FullName+'.sha256') -Raw).Split(' ')[0].Trim()
if ($sidecar -ne $packageHash) {throw 'Package checksum mismatch.'}
Expand-Archive -LiteralPath $package.FullName -DestinationPath (Join-Path $destination 'package') -Force
$executables=@(Get-ChildItem -LiteralPath (Join-Path $destination 'package') -Recurse -File -Filter TLauncher.exe)
if ($executables.Count -ne 1) {throw 'Ambiguous package binary.'}
$exe=$executables[0]
$commit=(Get-Content -LiteralPath (Join-Path $exe.DirectoryName 'BUILD-COMMIT.txt') -Raw).Trim()
$version=(Get-Content -LiteralPath (Join-Path $exe.DirectoryName 'VERSION.txt') -Raw).Trim()
if ($commit -ne $ExpectedCommit -or $version -ne '1.0.3' -or $exe.VersionInfo.ProductVersion -ne $version) {throw 'Package commit or version mismatch.'}
$report=[ordered]@{workflowRun=$RunId;workflow=$run.path;commit=$commit;version=$version;archives=$archives;
    packageSha256=$packageHash;exeSha256=(Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash.ToLower();
    ownerCandidate=$false;executed=$false;testShortcutUpdated=$false}
$report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $destination 'verified-internal-artifact.json') -Encoding UTF8
Write-Output ('INTERNAL_EVIDENCE_VERIFIED '+$destination)
