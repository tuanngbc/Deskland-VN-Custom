param(
    [Parameter(Mandatory=$true)][long]$RunId,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$ExpectedCommit,
    [Parameter(Mandatory=$true)][string]$EvidenceRoot
)
$ErrorActionPreference='Stop'
# Downloads and inspects only. A successful reference job may belong to an
# unfinished/failed TLauncher run; preserve that distinction in the evidence.
$api='https://api.github.com/repos/tuanngbc/TLauncher'
$run=Invoke-RestMethod "$api/actions/runs/$RunId"
if ($run.head_sha -ne $ExpectedCommit -or $run.path -ne '.github/workflows/build-rust-test.yml' -or
    $run.event -notin @('push','workflow_dispatch')) {throw 'Unexpected workflow or source commit.'}
$jobs=Invoke-RestMethod "$api/actions/runs/$RunId/jobs"
$job=@($jobs.jobs | Where-Object {$_.name -eq 'fluidwin-baseline'})
if ($job.Count -ne 1 -or $job[0].conclusion -ne 'success' -or $job[0].labels -contains 'self-hosted') {
    throw 'No unique successful hosted reference job.'
}
$listing=Invoke-RestMethod $run.artifacts_url
$selected=@($listing.artifacts | Where-Object {$_.name -eq 'fluidwin-reference-baseline'})
if ($selected.Count -ne 1 -or $selected[0].expired) {throw 'Reference artifact missing or expired.'}
$artifact=$selected[0]
if ($artifact.digest -notmatch '^sha256:[0-9a-f]{64}$') {throw 'Reference artifact checksum is unavailable.'}
$destination=Join-Path $EvidenceRoot ('reference-'+$ExpectedCommit.Substring(0,7)+'-'+$RunId)
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$archive=Join-Path $destination 'fluidwin-reference-baseline.zip'
if (-not (Test-Path -LiteralPath $archive -PathType Leaf) -or
    ('sha256:'+(Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()) -ne $artifact.digest) {
    $credentialFields=("protocol=https`nhost=github.com`n`n" | git credential fill)
    $credentialValue=@($credentialFields | Where-Object {$_.StartsWith('password=')})
    if ($credentialValue.Count -ne 1) {throw 'GitHub artifact access is unavailable.'}
    $headers=@{Authorization=('Bearer '+$credentialValue[0].Substring(9))}
    Invoke-WebRequest -Uri $artifact.archive_download_url -Headers $headers -OutFile $archive
}
$archiveHash=(Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ('sha256:'+$archiveHash -ne $artifact.digest) {throw 'Reference archive checksum mismatch.'}
$unpacked=Join-Path $destination 'unpacked'
Expand-Archive -LiteralPath $archive -DestinationPath $unpacked -Force
$reference=(Get-Content -LiteralPath (Join-Path $unpacked 'UPSTREAM-COMMIT.txt') -Raw).Trim()
if ($reference -ne '2f773a8733b84c7dbee88fff6628d14e9ac802b9') {throw 'Wrong upstream reference commit.'}
$binary=Get-Item -LiteralPath (Join-Path $unpacked 'FluidWin-reference.exe')
if ($binary.VersionInfo.ProductVersion -ne '1.3.1') {throw 'Wrong reference PE version.'}
$binaryHash=(Get-FileHash -LiteralPath $binary.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
$sidecar=(Get-Content -LiteralPath ($binary.FullName+'.sha256') -Raw).Trim()
if ($sidecar -notmatch ('^'+$binaryHash+'\s')) {throw 'Reference executable checksum mismatch.'}
$report=Get-Content -LiteralPath (Join-Path $unpacked 'baseline.txt')
$capture=@($report | Where-Object {$_ -match '^capture='})
$staticCapture=@($report | Where-Object {$_ -match '^static_'})
$proof=[ordered]@{workflowRun=$RunId;workflow=$run.path;workflowSource=$ExpectedCommit;referenceCommit=$reference;
    version=$binary.VersionInfo.ProductVersion;archiveSha256=$archiveHash;exeSha256=$binaryHash;artifactId=$artifact.id;
    referenceJob='success';fullRunStatus=$run.status;fullRunConclusion=$run.conclusion;captureReport=$capture;staticCaptureReport=$staticCapture;
    ownerCandidate=$false;executedLocally=$false;testShortcutUpdated=$false}
$proof | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $destination 'verified-reference-artifact.json') -Encoding UTF8
$report | Select-Object -Last 24
Write-Output ('REFERENCE_EVIDENCE_VERIFIED '+$destination)
