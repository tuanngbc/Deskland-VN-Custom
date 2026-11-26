@echo off
setlocal
echo Native Base TEST resource sampler. Open the TEST shortcut first.
echo Press a key to sample 60 seconds of normal use; close to cancel.
pause >nul
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0native-resource-diagnostics.ps1" -Seconds 60
pause
