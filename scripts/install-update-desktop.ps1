param([ValidateSet('tauri', 'clean', 'electron')][string]$Scenario)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:FABER_SMOKE_DISPOSABLE -ne '1') {
    throw 'Only a disposable GitHub Windows runner may run this proof.'
}
# All descendants inherit this desktop. Never switch to it, and never enumerate or stop another desktop's processes.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class ProofDesktop {
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct Startup {
    public int cb; public string reserved, desktop, title;
    public int x,y,width,height,charsX,charsY,fill,flags; public short show, reserved2;
    public IntPtr bytes, stdin, stdout, stderr;
  }
  [StructLayout(LayoutKind.Sequential)] public struct ProcessInfo { public IntPtr process, thread; public int pid, tid; }
  [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern IntPtr CreateDesktop(string name, IntPtr device, IntPtr mode, int flags, uint access, IntPtr security);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool CreateProcess(string app, string command, IntPtr ps, IntPtr ts, bool inherit, uint flags, IntPtr env, string cwd, ref Startup startup, out ProcessInfo process);
  [DllImport("kernel32.dll")] public static extern uint WaitForSingleObject(IntPtr handle, uint timeout);
  [DllImport("kernel32.dll")] public static extern bool GetExitCodeProcess(IntPtr handle, out uint code);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
  [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr handle);
}
'@
$desktopName = 'faber-proof-' + [Guid]::NewGuid().ToString('N')
$desktop = [ProofDesktop]::CreateDesktop($desktopName, [IntPtr]::Zero, [IntPtr]::Zero, 0, 0x1ff, [IntPtr]::Zero)
if ($desktop -eq [IntPtr]::Zero) { throw "CreateDesktop: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" }
$env:FABER_PROOF_DESKTOP = $desktopName
$startup = New-Object ProofDesktop+Startup
$startup.cb = [Runtime.InteropServices.Marshal]::SizeOf($startup)
$startup.desktop = "winsta0\$desktopName"
$startup.flags = 1
$startup.show = 0
$info = New-Object ProofDesktop+ProcessInfo
$node = (Get-Command node.exe).Source
$script = Join-Path $PWD 'scripts/install-update-self.mjs'
try {
    # CREATE_NO_WINDOW | IDLE_PRIORITY_CLASS. No shell is involved.
    if (![ProofDesktop]::CreateProcess($node, "`"$node`" `"$script`" $Scenario", [IntPtr]::Zero, [IntPtr]::Zero, $false, 0x08000040, [IntPtr]::Zero, $PWD.Path, [ref]$startup, [ref]$info)) {
        throw "CreateProcess: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    [void][ProofDesktop]::WaitForSingleObject($info.process, [uint32]::MaxValue)
    [uint32]$code = 1
    if (![ProofDesktop]::GetExitCodeProcess($info.process, [ref]$code)) { throw 'No proof exit code' }
    $evidence = Join-Path $env:RUNNER_TEMP 'install-update-evidence/result.json'
    if (Test-Path -LiteralPath $evidence) { Get-Content -LiteralPath $evidence }
    exit $code
} finally {
    if ($info.thread -ne [IntPtr]::Zero) { [void][ProofDesktop]::CloseHandle($info.thread) }
    if ($info.process -ne [IntPtr]::Zero) { [void][ProofDesktop]::CloseHandle($info.process) }
    [void][ProofDesktop]::CloseDesktop($desktop)
}
