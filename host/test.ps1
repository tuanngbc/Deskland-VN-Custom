$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'build.ps1')
$csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$bin = Join-Path $PSScriptRoot 'bin'
$lib = Join-Path $PSScriptRoot 'lib'
& $csc /nologo /target:exe /main:RegressionTests /platform:x64 /codepage:65001 /out:"$bin\RegressionTests.exe" `
  /r:System.dll /r:System.Drawing.dll /r:System.Windows.Forms.dll /r:System.Web.Extensions.dll `
  /r:"$lib\Microsoft.Web.WebView2.Core.dll" /r:"$lib\Microsoft.Web.WebView2.WinForms.dll" `
  "$PSScriptRoot\DesktopHost.cs" "$PSScriptRoot\Themes.cs" "$PSScriptRoot\MultiMonitor.cs" "$PSScriptRoot\NativeAcceptance.cs" "$PSScriptRoot\AwayMode.cs" "$PSScriptRoot\ScreenSaver.cs" "$PSScriptRoot\RegressionTests.cs"
if ($LASTEXITCODE -ne 0) { throw 'Test compilation failed' }
$oldProfile = $env:DESKLAND_PROFILE_DIR
try {
  $env:DESKLAND_PROFILE_DIR = Join-Path ([IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\artifacts'))) ('regression-' + [Guid]::NewGuid().ToString('N'))
  & "$bin\RegressionTests.exe"
  if ($LASTEXITCODE -ne 0) { throw 'Regression tests failed' }
  & node (Join-Path $PSScriptRoot '..\tests\renderer-contract.cjs')
  if ($LASTEXITCODE -ne 0) { throw 'Renderer tests failed' }
} finally { $env:DESKLAND_PROFILE_DIR = $oldProfile }
