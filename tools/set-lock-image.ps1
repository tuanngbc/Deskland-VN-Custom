param([string]$ImagePath,[switch]$ProbeOnly)
$ErrorActionPreference='Stop'
try {
    Add-Type -AssemblyName System.Runtime.WindowsRuntime
    $null=[Windows.Storage.StorageFile,Windows.Storage,ContentType=WindowsRuntime]
    $null=[Windows.System.UserProfile.LockScreen,Windows.System.UserProfile,ContentType=WindowsRuntime]
    $methods=[System.WindowsRuntimeSystemExtensions].GetMethods()
    $operationTask=@($methods | Where-Object { $_.Name -eq 'AsTask' -and $_.IsGenericMethodDefinition -and $_.GetGenericArguments().Count -eq 1 -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
    $actionTask=@($methods | Where-Object { $_.Name -eq 'AsTask' -and -not $_.IsGenericMethod -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.FullName -eq 'Windows.Foundation.IAsyncAction' })[0]
    if($null -eq $operationTask -or $null -eq $actionTask) { throw 'Windows lock-screen API is unavailable.' }
    if($ProbeOnly) { Write-Output 'Windows lock-image API available';exit 0 }
    if(-not (Test-Path -LiteralPath $ImagePath -PathType Leaf)) { throw 'The prepared Deskland scene image is missing.' }
    $fullPath=[IO.Path]::GetFullPath($ImagePath)
    $fileTask=$operationTask.MakeGenericMethod([Windows.Storage.StorageFile]).Invoke($null,@([Windows.Storage.StorageFile]::GetFileFromPathAsync($fullPath)))
    $fileTask.Wait()
    $update=[Windows.System.UserProfile.LockScreen]::SetImageFileAsync($fileTask.Result)
    $task=$actionTask.Invoke($null,@($update))
    $task.Wait()
    Write-Output 'Deskland image applied to Windows lock screen'
    exit 0
} catch {
    [Console]::Error.WriteLine($_.Exception.GetBaseException().Message)
    exit 1
}
