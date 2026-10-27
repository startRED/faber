param([int]$WindowPid, [string]$Mode = 'window')
$ErrorActionPreference = 'Stop'
$ids = @($WindowPid)
if ($Mode -eq 'tree') {
    $all = @(Get-CimInstance Win32_Process)
    for ($i = 0; $i -lt $ids.Count; $i++) {
        $parent = $ids[$i]
        $ids += @($all | Where-Object { $_.ParentProcessId -eq $parent -and $_.ProcessId -notin $ids } | ForEach-Object { [int]$_.ProcessId })
    }
}
$samples = (Get-Counter '\GPU Process Memory(*)\Dedicated Usage','\GPU Process Memory(*)\Shared Usage').CounterSamples | Where-Object { $_.InstanceName -match '^pid_(\d+)_' -and [int]$Matches[1] -in $ids }
if (-not $samples) { throw "No GPU Process Memory instances for PID $WindowPid" }
$dedicated = ($samples | Where-Object { $_.Path -match '\\dedicated usage$' } | Measure-Object -Property CookedValue -Sum).Sum
$shared = ($samples | Where-Object { $_.Path -match '\\shared usage$' } | Measure-Object -Property CookedValue -Sum).Sum
if ($null -eq $dedicated -or $null -eq $shared) { throw 'Missing GPU counter' }
'gpu_dedicated_mb={0:F2} gpu_shared_mb={1:F2}' -f ($dedicated / 1MB), ($shared / 1MB)
