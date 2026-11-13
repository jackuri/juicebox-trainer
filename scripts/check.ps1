# Runs every check CI runs, in the same order. Usage: npm run check
#Requires -Version 5.1
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)

function Invoke-Step([string]$Name, [scriptblock]$Command) {
    Write-Host "==> $Name" -ForegroundColor Cyan
    & $Command
    if ($LASTEXITCODE -ne 0) { throw "$Name failed (exit $LASTEXITCODE)" }
}

if (-not (Test-Path node_modules)) { Invoke-Step 'npm ci' { npm ci } }
Invoke-Step 'Build web UI'            { npm run build:web }
Invoke-Step 'Build AI helper'         { npm run build:agent }
Invoke-Step 'cargo fmt'               { cargo fmt --all --check }
Invoke-Step 'cargo clippy (x64)'      { cargo clippy --workspace --all-targets -- -D warnings }
Invoke-Step 'cargo clippy (testbed x86)' { cargo clippy -p testbed-native --target i686-pc-windows-msvc --all-features -- -D warnings }
Invoke-Step 'cargo test'              { cargo test --workspace }
Invoke-Step 'Frontend lint'           { npm run lint }
Invoke-Step 'Frontend typecheck'      { npm run typecheck }
Invoke-Step 'Frontend tests'          { npm test }
Invoke-Step 'Website'                 { npm run site }

Write-Host 'All checks passed.' -ForegroundColor Green
