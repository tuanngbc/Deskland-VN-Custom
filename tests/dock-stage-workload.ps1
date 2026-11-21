function Test-DockStageWorkload([string[]]$Lines, [double]$From, [double]$Through, [int]$Expected) {
    $expectedState = "$Expected 0"
    $initial = $null
    foreach ($line in $Lines) {
        if ($line -cmatch '^\[(\d+\.\d+)\] DOCK_BENCH_STAGE (\d+ \d+)$') {
            $stamp = [double]::Parse($Matches[1], [Globalization.CultureInfo]::InvariantCulture)
            if ($stamp -le $From) { $initial = $Matches[2] }
            elseif ($stamp -le $Through -and $Matches[2] -cne $expectedState) { return $false }
        }
    }
    return $initial -ceq $expectedState
}
