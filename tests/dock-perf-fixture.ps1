param(
    [Parameter(Mandatory)][string]$Evidence,
    [ValidateRange(0,8)][int]$Cards = 0,
    [ValidateSet('none','primary','secondary')][string]$Fullscreen = 'none',
    [ValidateSet('primary','secondary')][string[]]$BackgroundCovered = @(),
    [ValidateRange(30,900)][int]$TimeoutSeconds = 180
)
$ErrorActionPreference = 'Stop'
if (!(Test-Path -LiteralPath $Evidence -PathType Container)) { throw 'Existing private evidence folder required' }
if (Test-Path -LiteralPath (Join-Path $Evidence 'fixture-ready.json')) { throw 'Select an unused evidence folder' }
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DeskPerfFixture {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
'@
[void][DeskPerfFixture]::SetProcessDpiAwarenessContext([IntPtr](-4))
$screens = @([Windows.Forms.Screen]::AllScreens | Sort-Object Primary -Descending)
if ($screens.Count -ne 2) { throw 'This scenario requires two physical displays' }
$forms = [Collections.Generic.List[Windows.Forms.Form]]::new()
$clock = [Diagnostics.Stopwatch]::StartNew()
$stop = Join-Path $Evidence 'fixture-stop'
try {
    # Test-owned, ordinary small windows. No resizing or hiding unrelated apps.
    for ($i=0; $i -lt $Cards; $i++) {
        $screen = $screens[$i % 2]
        $form = [Windows.Forms.Form]::new()
        $forms.Add($form)
        $form.Text = "Bench card $i"
        $form.StartPosition = 'Manual'
        $form.Bounds = [Drawing.Rectangle]::new($screen.Bounds.Left+205+($i%4)*24,$screen.Bounds.Top+160+($i%4)*58,365,225)
        $form.BackColor = [Drawing.Color]::FromArgb(42+$i*12,101,154)
        # WinForms uses a layered source when Opacity is below 1.0. This
        # permits a verified display-affinity query on Windows 11.
        $form.Opacity = 0.98
        $form.Show()
    }
    $control = [Windows.Forms.Form]::new()
    $forms.Add($control)
    $control.Text = 'Deskland isolated benchmark fixture'
    $control.StartPosition = 'Manual'
    $control.BackColor = [Drawing.Color]::MidnightBlue
    $deactivated = Join-Path $Evidence 'fixture-deactivated.log'
    $control.Add_Deactivate({
        [IO.File]::AppendAllText($deactivated, [string][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()+[Environment]::NewLine)
    }.GetNewClosure())
    $screen = if ($Fullscreen -eq 'secondary') { $screens[1] } else { $screens[0] }
    if ($Fullscreen -eq 'none') {
        $control.Bounds = [Drawing.Rectangle]::new($screen.Bounds.Left+475,$screen.Bounds.Top+100,400,230)
    } else {
        $control.FormBorderStyle = 'None'
        $control.Bounds = $screen.Bounds
    }
    $control.Show()
    $control.Activate()
    $next = Join-Path $Evidence 'fixture-ready.next'
    $ready = Join-Path $Evidence 'fixture-ready.json'
    $expected = ($screens | Sort-Object {$_.Bounds.Left}, {$_.Bounds.Top} | ForEach-Object {
        $b = $_.Bounds
        $side = if ($_.Primary) { 'primary' } else { 'secondary' }
        $covered = ($side -eq $Fullscreen) -or ($side -in $BackgroundCovered)
        '{0},{1},{2},{3}:{4}' -f $b.Left,$b.Top,$b.Right,$b.Bottom,[int]$covered
    }) -join ';'
    [ordered]@{PID=$PID;Hwnd=$control.Handle.ToInt64();Cards=$Cards;Coverage=$expected;Fullscreen=$Fullscreen;BackgroundCovered=$BackgroundCovered} |
        ConvertTo-Json | Set-Content -LiteralPath $next
    Move-Item -LiteralPath $next -Destination $ready
    while ($clock.Elapsed.TotalSeconds -lt $TimeoutSeconds -and !(Test-Path -LiteralPath $stop) -and !$control.IsDisposed) {
        [Windows.Forms.Application]::DoEvents()
        Start-Sleep -Milliseconds 30
    }
} finally {
    foreach ($form in $forms) { $form.Dispose() }
}
