param(
    [Parameter(Mandatory)][string]$Directory,
    [switch]$Primary,
    [switch]$Anchors
)

$ErrorActionPreference = 'Stop'
$directory = (Resolve-Path -LiteralPath $Directory).Path
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DesklandVisualFixture {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool SetWindowDisplayAffinity(IntPtr window, uint affinity);
}
'@
[void][DesklandVisualFixture]::SetProcessDpiAwarenessContext([IntPtr](-4))
$screen = [Windows.Forms.Screen]::AllScreens | Where-Object { $_.Primary -eq [bool]$Primary } | Select-Object -First 1
if (!$screen) { throw 'Requested fixture display not found' }
$side = if ($Primary) {'Landscape'} else {'Portrait'}
$forms = [ordered]@{}
$normal = @{}
$protected = $false
$anchorWindows = @()
try {
    foreach ($name in 'Slate','Lagoon','Coral','Protected','Pre-minimized') {
        $i = $forms.Count
        $form = [Windows.Forms.Form]::new()
        $form.Text = "Deskland Fixture $side $name"
        $form.StartPosition = 'Manual'
        $normal[$name] = [Drawing.Rectangle]::new($screen.WorkingArea.X + 240 + ($i % 3) * 66, $screen.WorkingArea.Y + 145 + $i * 82, 520, 265)
        $form.Bounds = $normal[$name]
        $form.BackColor = [Drawing.Color]::FromArgb(@(0x243549,0x146A74,0x903C59,0x514779,0x713D58)[$i] -bor -16777216)
        $label = [Windows.Forms.Label]::new()
        $label.Dock = 'Fill'
        $label.TextAlign = 'MiddleCenter'
        $label.ForeColor = [Drawing.Color]::White
        $label.Font = [Drawing.Font]::new('Segoe UI',23)
        $label.Text = "$name`r`nDeskland visual fixture"
        $form.Controls.Add($label)
        if ($name -eq 'Protected') { $protected = [DesklandVisualFixture]::SetWindowDisplayAffinity($form.Handle,0x11) }
        $form.Show()
        # Show can apply the initial STARTUPINFO; apply the iconic state afterwards.
        if ($name -eq 'Pre-minimized') { $form.WindowState = 'Minimized' }
        $forms[$name] = $form
    }

    if ($Anchors) {
        $work = $screen.WorkingArea
        $bottomWidth = [Math]::Min(924, $work.Width - 24)
        $stageX = if ($Primary) { $work.X } else { $work.Right - 244 }
        foreach ($edge in 'Dock', 'Stage') {
            $anchor = [Windows.Forms.Form]::new()
            $anchor.Text = "Deskland visual anchor $side $edge"
            $anchor.StartPosition = 'Manual'
            $anchor.FormBorderStyle = 'FixedSingle'
            $anchor.BackColor = if ($Primary) { [Drawing.Color]::FromArgb(219,235,234) } else { [Drawing.Color]::FromArgb(33,51,72) }
            $anchor.Bounds = if ($edge -eq 'Dock') {
                [Drawing.Rectangle]::new($work.X + [int](($work.Width-$bottomWidth)/2),$work.Bottom - 150,$bottomWidth,150)
            } else {
                [Drawing.Rectangle]::new($stageX,$work.Y + 36,244,$work.Height - 190)
            }
            $anchor.Show()
            $anchorWindows += $anchor
        }
    }

    function Report-Fixture($file) {
        $rows = @(foreach ($name in $forms.Keys) {
            $form = $forms[$name]
            if ($form.IsDisposed) { continue }
            $r = $form.Bounds
            [pscustomobject]@{Name=$name;Title=$form.Text;Handle=$form.Handle.ToInt64();State=$form.WindowState.ToString();Rect=@($r.Left,$r.Top,$r.Right,$r.Bottom)}
        })
        $r = $screen.Bounds; $work = $screen.WorkingArea
        [pscustomobject]@{Pid=$PID;Primary=[bool]$Primary;Protected=$protected;Monitor=@($r.Left,$r.Top,$r.Right,$r.Bottom);Work=@($work.Left,$work.Top,$work.Right,$work.Bottom);Windows=$rows} |
            ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $directory $file) -Encoding utf8
    }

    [Windows.Forms.Application]::DoEvents()
    Report-Fixture 'fixture-ready.json'
    $until = [DateTime]::UtcNow.AddMinutes(15)
    $seq = 0
    $file = Join-Path $directory 'fixture-action.json'
    while ([DateTime]::UtcNow -lt $until -and !(Test-Path -LiteralPath (Join-Path $directory 'fixture-stop'))) {
        [Windows.Forms.Application]::DoEvents()
        if (Test-Path -LiteralPath $file) {
            try { $request = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json } catch { $request = $null }
            if ($request -and [int]$request.sequence -gt $seq -and $forms.Contains([string]$request.name)) {
                $seq = [int]$request.sequence
                $target = $forms[[string]$request.name]
                if (!$target.IsDisposed) {
                    switch ($request.action) {
                        'color' { $target.BackColor = [Drawing.Color]::SeaGreen; $target.Controls[0].Text = 'Live fixture frame changed' }
                        'maximize' { $target.WindowState = 'Maximized' }
                        'restore' { $target.WindowState = 'Normal'; $target.TopMost = $false; $target.FormBorderStyle = 'Sizable'; $target.Bounds = $normal[$request.name] }
                        'minimize' { $target.WindowState = 'Minimized' }
                        'borderless' { $target.WindowState = 'Normal'; $target.FormBorderStyle = 'None'; $target.Bounds = $screen.Bounds; $target.TopMost = $true; $target.Activate() }
                        'close' { $target.Close() }
                    }
                }
                [Windows.Forms.Application]::DoEvents()
                Report-Fixture "fixture-action-$seq.json"
            }
        }
        Start-Sleep -Milliseconds 30
    }
} finally {
    foreach ($form in $forms.Values) { if (!$form.IsDisposed) { $form.Close(); $form.Dispose() } }
    foreach ($anchor in $anchorWindows) { if (!$anchor.IsDisposed) { $anchor.Close(); $anchor.Dispose() } }
}
