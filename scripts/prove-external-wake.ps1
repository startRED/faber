#Requires -Version 7.0
<#
.SYNOPSIS
Exercises the desktop command and an isolated, windowless Faber command host.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $CliPath,
    [Parameter(Mandatory)] [string] $TestHostPath
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This proof requires Windows.' }
$cli = (Resolve-Path -LiteralPath $CliPath).ProviderPath
$artifact = (Resolve-Path -LiteralPath $TestHostPath).ProviderPath
$label = 'external-wake-' + [guid]::NewGuid().ToString('N')
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "../.faber/verify/$label"))
$data = Join-Path $root 'data'
$project = Join-Path $root 'project'
$conversations = Join-Path $data 'conversations'
[IO.Directory]::CreateDirectory($conversations) | Out-Null
[IO.Directory]::CreateDirectory($project) | Out-Null
$app = Join-Path $root 'faber-external-wake.exe'
$receiptFile = Join-Path $root 'receipt.json'
Copy-Item -LiteralPath $artifact -Destination $app
$receipt = [ordered]@{ passed = $false; observed = [ordered]@{} }

function Write-Json([string] $Path, $Value) {
    [IO.File]::WriteAllText($Path, (ConvertTo-Json -InputObject $Value -Depth 12))
}

function Get-TestApps {
    Get-Process -Name 'faber-external-wake' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $script:app }
}

function Stop-TestApps {
    $issues = @()
    foreach ($appProcess in @(Get-TestApps)) {
        try {
            $appProcess.Kill($true)
            if (-not $appProcess.WaitForExit(6000)) { throw 'The isolated app did not exit.' }
        } catch { $issues += $_.Exception.Message }
        finally { $appProcess.Dispose() }
    }
    if ($issues.Count -gt 0) { throw ($issues -join '; ') }
}

function Invoke-Faber([string[]] $Words, [int] $ExpectedCode) {
    $launch = [Diagnostics.ProcessStartInfo]::new($cli)
    $launch.WorkingDirectory = $project
    $launch.UseShellExecute = $false
    $launch.CreateNoWindow = $true
    $launch.RedirectStandardOutput = $true
    $launch.RedirectStandardError = $true
    foreach ($name in @($launch.Environment.Keys | Where-Object { $_ -like 'FABER_*' })) {
        $launch.Environment.Remove($name) | Out-Null
    }
    $launch.Environment['FABER_APP'] = $app
    $launch.Environment['FABER_DATA_DIR'] = $data
    $launch.Environment['FABER_HIDDEN'] = '1'
    # Explicit invalid CLI descriptions prevent discovery of the real account programs.
    $launch.Environment['FABER_CLI_CLAUDE'] = 'null'
    $launch.Environment['FABER_CLI_CODEX'] = 'null'
    foreach ($word in (@('--data', $data, '--json') + $Words)) { $launch.ArgumentList.Add($word) }
    $process = [Diagnostics.Process]::Start($launch)
    try {
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(16000)) {
            $process.Kill()
            if (-not $process.WaitForExit(6000)) { throw 'The test command could not be stopped.' }
            throw 'The test command exceeded 16 seconds.'
        }
        if (-not [Threading.Tasks.Task]::WaitAll(@($stdout, $stderr), 3000)) {
            throw 'The CLI exited but its stdout or stderr remained open.'
        }
        $out = $stdout.GetAwaiter().GetResult()
        $err = $stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne $ExpectedCode) { throw "Unexpected CLI exit $($process.ExitCode): $out $err" }
        return ($out | ConvertFrom-Json)
    } finally { $process.Dispose() }
}

function Assert-Value($Actual, $Expected, [string] $Check) {
    if ($Actual -cne $Expected) { throw "$Check expected '$Expected', got '$Actual'." }
}

function Assert-HiddenApp {
    $apps = @(Get-TestApps)
    if ($apps.Count -ne 1) {
        $candidates = @(Get-Process -Name 'faber-external-wake' -ErrorAction SilentlyContinue).Count
        throw "Expected one isolated app process, found $($apps.Count) matching the fixture path out of $candidates candidates."
    }
    $apps[0].Refresh()
    Assert-Value $apps[0].MainWindowHandle ([IntPtr]::Zero) 'No visible main window'
    return $apps[0].Id
}

$id = 'external-wait'
$savedFile = Join-Path $conversations "$id.json"
$now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$since = $now - 1000
$until = $now + 22000
$wake = @('wake', $id, '--since', "$since", '--until', "$until")
$receipt.wait = [ordered]@{ since = $since; until = $until }
[IO.File]::WriteAllText((Join-Path $data 'external-wake-fixture'), $label)
Write-Json (Join-Path $data 'projects.json') @(@{ id = 'test-project'; name = 'Supervisor proof'; path = $project })
Write-Json $savedFile @{
    id = $id; title = 'Synthetic supervisor wait'; area = 'project'; projectId = 'test-project'
    createdAt = $since; updatedAt = $since; sessions = @{}; queued = @()
    waitingOn = @{ note = 'Wait recorded before the app closed'; since = $since; until = $until }
    messages = @(
        @{ id = 'question'; role = 'user'; text = 'Record the wait.'; at = $since }
        @{ id = 'reply'; role = 'assistant'; text = 'Waiting for the result.'; at = $since; vendor = 'claude'; status = 'done' }
    )
}

$checked = $false
try {
    $receipt.observed.closed = Invoke-Faber (@('--no-start') + $wake) 3
    Assert-Value $receipt.observed.closed.error 'cli.error.noFaber' 'Result with startup disabled'
    Assert-Value (@(Get-TestApps).Count) 0 'App must remain closed'

    $receipt.observed.early = Invoke-Faber $wake 0
    Assert-Value $receipt.observed.early.state 'tooSoon' 'Result before deadline'
    Assert-Value $receipt.observed.early.until $until 'Earlier observation retained the deadline'
    $receipt.firstPid = Assert-HiddenApp
    Stop-TestApps
    Assert-Value (@(Get-TestApps).Count) 0 'App exited before the deadline'
    $remaining = $until - [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    if ($remaining -le 0) { throw 'The first app stopped too late to prove recovery.' }
    # Drive the real saved deadline; no clocks or data are changed between the two opens.
    [Threading.Tasks.Task]::Delay([int] ($remaining + 100)).GetAwaiter().GetResult()

    $receipt.observed.overdue = Invoke-Faber $wake 0
    Assert-Value $receipt.observed.overdue.state 'blocked' 'Result of the same overdue wait'
    Assert-Value $receipt.observed.overdue.reason 'error.noSubscription' 'Reason for not starting a model'
    $receipt.secondPid = Assert-HiddenApp
    if ($receipt.secondPid -eq $receipt.firstPid) { throw 'Expected a different app process after restart.' }
    $receipt.observed.stale = Invoke-Faber @('--no-start', 'wake', $id, '--since', "$since", '--until', "$($until + 1000)") 0
    Assert-Value $receipt.observed.stale.state 'obsolete' 'Different deadline cannot check this wait'

    $saved = Get-Content -Raw -LiteralPath $savedFile | ConvertFrom-Json
    Assert-Value $saved.waitingOn.until $until 'Restored deadline'
    Assert-Value $saved.waitingOn.problem 'error.noSubscription' 'Restored blocker'
    Assert-Value $saved.messages.Count 2 'No extra model turn or wake receipt'
    $checked = $true
} catch {
    $receipt.error = $_.Exception.Message
    throw
} finally {
    try {
        Stop-TestApps
        Remove-Item -LiteralPath $app
        $receipt.passed = $checked
    } catch {
        $receipt.cleanupError = $_.Exception.Message
        throw
    } finally {
        Write-Json $receiptFile $receipt
        Write-Host "Proof receipt: $receiptFile"
    }
}
