# Used by "Try OpenTrainer.cmd" before starting the app.
# - Stops a leftover OpenTrainer dev web server (it holds port 5173 and the app can't start).
# - Exits with 2 if OpenTrainer from this folder is already open.
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

$app = Get-CimInstance Win32_Process -Filter "Name='opentrainer-desktop.exe'" |
    Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($root, 'OrdinalIgnoreCase') }
if ($app) {
    Write-Host 'OpenTrainer is already open. Look for its window on the taskbar.'
    exit 2
}

$vite = Join-Path $root 'node_modules\vite\bin\vite.js'
$leftover = Get-CimInstance Win32_Process -Filter "Name='node.exe'" | Where-Object {
    $cmd = $_.CommandLine
    if (-not $cmd -or $cmd -notmatch 'vite[\\/]bin[\\/]vite\.js') { return $false }
    # Normalise "node_modules\.bin\\..\vite" (npm's shim) to "node_modules\vite".
    $cmd = ($cmd -replace '[\\/]+', '\') -replace '\\\.bin\\\.\.\\', '\'
    $cmd.IndexOf($vite, [StringComparison]::OrdinalIgnoreCase) -ge 0
}
foreach ($p in $leftover) {
    Write-Host "Stopping a leftover OpenTrainer web server (process $($p.ProcessId))..."
    Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
}
if ($leftover) { Start-Sleep -Milliseconds 500 }

$busy = Get-NetTCPConnection -LocalPort 5173 -State Listen -ErrorAction SilentlyContinue
if ($busy) {
    $name = (Get-Process -Id $busy[0].OwningProcess -ErrorAction SilentlyContinue).ProcessName
    Write-Host "Another program ($name) is using port 5173, which OpenTrainer needs. Close it and try again."
    exit 1
}
exit 0
