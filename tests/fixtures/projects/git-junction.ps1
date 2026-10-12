param([string]$Link, [string]$Target)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Junction -Path $Link -Target $Target | Out-Null
