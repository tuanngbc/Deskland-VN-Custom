param(
    [Parameter(Mandatory)][string]$Evidence,
    [ValidateRange(0,8)][int]$Cards = 0,
    [ValidateSet('none','primary','secondary')][string]$Fullscreen = 'none',
    [ValidateSet('primary','secondary')][string[]]$BackgroundCovered = @(),
    [switch]$WaitForShow,
    [switch]$StageDiagnostic,
    [switch]$SourceSmoke,
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
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [StructLayout(LayoutKind.Sequential)] struct Rect { public int l,t,r,b; }
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll", EntryPoint="GetWindowLongW")] public static extern int GetWindowLong(IntPtr window, int index);
    [DllImport("user32.dll")] public static extern bool GetWindowDisplayAffinity(IntPtr window, out uint affinity);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr window, uint attribute, out uint value, uint size);
    public static string WindowRect(IntPtr window) {
        Rect r;
        if (!GetWindowRect(window, out r)) return null;
        return string.Format("{0},{1},{2},{3}",r.l,r.t,r.r,r.b);
    }
}
'@
[void][DeskPerfFixture]::SetProcessDpiAwarenessContext([IntPtr](-4))
$previousDpi = [DeskPerfFixture]::SetThreadDpiAwarenessContext([IntPtr](-4))
if ($previousDpi -eq [IntPtr]::Zero) { throw 'PMv2 fixture geometry unavailable' }
$screens = @([Windows.Forms.Screen]::AllScreens | Sort-Object Primary -Descending)
if ($SourceSmoke) {
    if (!$screens.Count -or $Cards -lt 1 -or $WaitForShow -or $Fullscreen -ne 'none') { throw 'Invalid source smoke configuration' }
} elseif ($screens.Count -ne 2) { throw 'This scenario requires two physical displays' }
$forms = [Collections.Generic.List[Windows.Forms.Form]]::new()
$clock = [Diagnostics.Stopwatch]::StartNew()
$stop = Join-Path $Evidence 'fixture-stop'
try {
    # Test-owned, ordinary small windows. No resizing or hiding unrelated apps.
    for ($i=0; $i -lt $Cards; $i++) {
        $screen = $screens[$i % $screens.Count]
        $form = [Windows.Forms.Form]::new()
        $forms.Add($form)
        $form.Text = "Bench card $i"
        $form.StartPosition = 'Manual'
        $row = [int][Math]::Floor($i / 2) % 2
        $column = [int][Math]::Floor($i / 4)
        $form.Bounds = [Drawing.Rectangle]::new($screen.Bounds.Left+205+$column*370,$screen.Bounds.Top+160+$row*240,355,215)
        $form.BackColor = [Drawing.Color]::FromArgb(42+$i*12,101,154)
        # WinForms uses a layered source when Opacity is below 1.0. This
        # permits a verified display-affinity query on Windows 11.
        $form.Opacity = 0.98
    }
    $control = [Windows.Forms.Form]::new()
    $forms.Add($control)
    $control.Text = 'Deskland isolated benchmark fixture'
    $control.StartPosition = 'Manual'
    $control.BackColor = [Drawing.Color]::MidnightBlue
    if ($Cards -gt 0 -and $Fullscreen -eq 'none') {
        # This owns foreground in both intervals without occupying a Stage
        # card or providing a fourth DWM source in the preview scenario.
        $control.FormBorderStyle = 'FixedToolWindow'
        $control.ShowInTaskbar = $false
    }
    $deactivated = Join-Path $Evidence 'fixture-deactivated.log'
    $control.Add_Deactivate({
        [IO.File]::AppendAllText($deactivated, [string][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()+[Environment]::NewLine)
    }.GetNewClosure())
    $screen = if ($Fullscreen -eq 'secondary') { $screens[1] } else { $screens[0] }
    if ($Fullscreen -eq 'none') {
        $offset = if ($Cards -gt 0) { [Math]::Min(605, $screen.Bounds.Width-400) } else { 475 }
        $control.Bounds = [Drawing.Rectangle]::new($screen.Bounds.Left+$offset,$screen.Bounds.Top+100,400,230)
    } else {
        $control.FormBorderStyle = 'None'
        $control.Bounds = $screen.Bounds
    }
    if ($WaitForShow) {
        # Publish the owned HWND while it is hidden, so the collector can bind
        # the candidate to this fixture without moving foreground before launch.
        $handle = $control.Handle.ToInt64()
        $created = Join-Path $Evidence 'fixture-created.next'
        $sourceHandles = if ($StageDiagnostic) { @(for ($i=0; $i -lt $Cards; $i++) { $forms[$i].Handle.ToInt64() }) }
        [ordered]@{PID=$PID;Hwnd=$handle;SourceHandles=@($sourceHandles)} | ConvertTo-Json | Set-Content -LiteralPath $created
        Move-Item -LiteralPath $created -Destination (Join-Path $Evidence 'fixture-created.json')
        $show = Join-Path $Evidence 'fixture-show'
        while (!(Test-Path -LiteralPath $show)) {
            if ($clock.Elapsed.TotalSeconds -ge 15 -or (Test-Path -LiteralPath $stop)) { throw 'Fixture show was not requested' }
            Start-Sleep -Milliseconds 50
        }
    }
    foreach ($form in $forms) { $form.Show() }
    if (!$SourceSmoke) { $control.Activate() }
    if ($Cards -gt 0) {
        [Windows.Forms.Application]::DoEvents()
        $sources = @(for ($i=0; $i -lt $Cards; $i++) {
            $source = $forms[$i].Handle
            if ($StageDiagnostic -and $source.ToInt64() -ne @($sourceHandles)[$i]) { throw "Fixture card $i recreated its HWND on show" }
            # A hidden launcher can override the first ShowWindow call with
            # SW_HIDE. Enforce visibility on the test-owned HWND without
            # activation, movement, resize, or raising it above other apps.
            $initiallyVisible = [DeskPerfFixture]::IsWindowVisible($source)
            if (!$initiallyVisible) {
                $flags = 0x0040 -bor 0x0010 -bor 0x0004 -bor 0x0002 -bor 0x0001
                if (![DeskPerfFixture]::SetWindowPos($source,[IntPtr]::Zero,0,0,0,0,$flags)) {
                    throw "Failed to show test card $i"
                }
            }
            [uint32]$affinity = 0
            [uint32]$cloaked = 0
            $affinityOk = [DeskPerfFixture]::GetWindowDisplayAffinity($source,[ref]$affinity)
            $cloakOk = [DeskPerfFixture]::DwmGetWindowAttribute($source,14,[ref]$cloaked,4) -ge 0
            [pscustomobject]@{
                Card=$i;Rect=[DeskPerfFixture]::WindowRect($source)
                Output=if([Windows.Forms.Screen]::FromHandle($source).Primary){'primary'}else{'secondary'}
                InitiallyVisible=$initiallyVisible;Visible=[DeskPerfFixture]::IsWindowVisible($source);Minimized=[DeskPerfFixture]::IsIconic($source)
                Unowned=[DeskPerfFixture]::GetWindow($source,4) -eq [IntPtr]::Zero
                ToolWindow=([DeskPerfFixture]::GetWindowLong($source,-20) -band 0x80) -ne 0
                AffinityOk=$affinityOk;Affinity=$affinity;CloakOk=$cloakOk;Cloaked=$cloaked
            }
        })
        ConvertTo-Json -InputObject $sources | Set-Content -LiteralPath (Join-Path $Evidence 'fixture-card-check.json')
        if (@($sources | Where-Object { !$_.Rect -or !$_.Visible -or $_.Minimized -or !$_.Unowned -or $_.ToolWindow -or
                    !$_.AffinityOk -or $_.Affinity -ne 0 -or !$_.CloakOk -or $_.Cloaked -ne 0 }).Count) {
            throw 'A test-owned card failed the Win32 preview source checks'
        }
    }
    if ($SourceSmoke) { return }
    $controlRect = [DeskPerfFixture]::WindowRect($control.Handle)
    if (!$controlRect -or $control.WindowState -ne 'Normal') { throw 'Expected ordinary owned fixture geometry' }
    $next = Join-Path $Evidence 'fixture-ready.next'
    $ready = Join-Path $Evidence 'fixture-ready.json'
    $expected = ($screens | Sort-Object {$_.Bounds.Left}, {$_.Bounds.Top} | ForEach-Object {
        $b = $_.Bounds
        $side = if ($_.Primary) { 'primary' } else { 'secondary' }
        $covered = ($side -eq $Fullscreen) -or ($side -in $BackgroundCovered)
        '{0},{1},{2},{3}:{4}' -f $b.Left,$b.Top,$b.Right,$b.Bottom,[int]$covered
    }) -join ';'
    [ordered]@{PID=$PID;Hwnd=$control.Handle.ToInt64();Rect=$controlRect;Cards=$Cards;Coverage=$expected;Fullscreen=$Fullscreen;BackgroundCovered=$BackgroundCovered} |
        ConvertTo-Json | Set-Content -LiteralPath $next
    Move-Item -LiteralPath $next -Destination $ready
    while ($clock.Elapsed.TotalSeconds -lt $TimeoutSeconds -and !(Test-Path -LiteralPath $stop) -and !$control.IsDisposed) {
        [Windows.Forms.Application]::DoEvents()
        Start-Sleep -Milliseconds 30
    }
} finally {
    foreach ($form in $forms) { $form.Dispose() }
    [void][DeskPerfFixture]::SetThreadDpiAwarenessContext($previousDpi)
}
