param([Parameter(Mandatory)][string]$Output)
$ErrorActionPreference = 'Stop'
# Read only. This does not mount the notification overflow, send messages to
# apps, register icons, read their names, or change Explorer's visibility.
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class TrayTraceWindow {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  [DllImport("user32.dll")] public static extern IntPtr GetShellWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  public delegate bool Callback(IntPtr h, IntPtr data);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback cb, IntPtr data);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int max);
}
"@
function Trace-Uia($root, $walker) {
    $queue = [Collections.Generic.Queue[object]]::new()
    $queue.Enqueue(@{Node=$root;Depth=0})
    $nodes = $buttons = $invoke = $errors = 0
    $depthLimit = $false
    while ($queue.Count -and $nodes -lt 512) {
        $item = $queue.Dequeue()
        $node = $item.Node
        ++$nodes
        try {
            if ($item.Depth -gt 0 -and $node.Current.ControlType -eq [Windows.Automation.ControlType]::Button) {
                ++$buttons
                $pattern = $null
                if ($node.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) { ++$invoke }
            }
            if ($item.Depth -ge 12) { $depthLimit = $true; continue }
            $child = $walker.GetFirstChild($node)
            while ($null -ne $child -and ($nodes+$queue.Count) -lt 512) {
                $queue.Enqueue(@{Node=$child;Depth=($item.Depth+1)})
                $child = $walker.GetNextSibling($child)
            }
        } catch { ++$errors }
    }
    [ordered]@{Descendants=($nodes-1);Buttons=$buttons;InvokePatterns=$invoke;ProviderErrors=$errors;Limited=($nodes -ge 512 -or $depthLimit)}
}
$classes = 'Shell_TrayWnd','Shell_SecondaryTrayWnd','TopLevelWindowForOverflowXamlIsland','NotifyIconOverflowWindow'
[uint32]$shell = 0
[void][TrayTraceWindow]::GetWindowThreadProcessId([TrayTraceWindow]::GetShellWindow(),[ref]$shell)
if (!$shell) { throw 'No Explorer shell HWND in the current desktop session' }
$windows = [Collections.Generic.List[object]]::new()
$collector = [TrayTraceWindow+Callback]{
    param([IntPtr]$h,[IntPtr]$ignored)
    $s = [Text.StringBuilder]::new(96)
    [void][TrayTraceWindow]::GetClassName($h,$s,$s.Capacity)
    $name = $s.ToString()
    if ($name -notin $classes) { return $true }
    [uint32]$pidOwner = 0
    [void][TrayTraceWindow]::GetWindowThreadProcessId($h,[ref]$pidOwner)
    if ($pidOwner -ne $shell) { return $true }
    $windows.Add(@{Handle=$h;Class=$name})
    return $true
}
[void][TrayTraceWindow]::EnumWindows($collector,[IntPtr]::Zero)
$traces = @(foreach ($class in $classes) {
    $matches = @($windows | Where-Object {$_.Class -eq $class})
    if ($matches.Count -eq 0) { [ordered]@{Class=$class;Present=$false}; continue }
    foreach ($entry in $matches) {
        $h = $entry.Handle
        $r = New-Object TrayTraceWindow+RECT
        $hasRect = [TrayTraceWindow]::GetWindowRect($h,[ref]$r)
        $root = $null
        $errorName = $null
        try { $root = [Windows.Automation.AutomationElement]::FromHandle($h) } catch { $errorName = $_.Exception.GetType().Name }
        [ordered]@{Class=$class;Present=$true;Visible=[TrayTraceWindow]::IsWindowVisible($h);
            Bounds=if ($hasRect) {@($r.L,$r.T,$r.R,$r.B)}else{$null};
            UiaRootAvailable=($null -ne $root);UiaError=$errorName;
            Raw=if ($root) {Trace-Uia $root ([Windows.Automation.TreeWalker]::RawViewWalker)}else{$null};
            Control=if ($root) {Trace-Uia $root ([Windows.Automation.TreeWalker]::ControlViewWalker)}else{$null}}
    }
})
$os = Get-CimInstance Win32_OperatingSystem
[ordered]@{Utc=[DateTime]::UtcNow.ToString('o');WindowsBuild=$os.BuildNumber;ReadOnly=$true;Windows=$traces} |
    ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Output -Encoding utf8
