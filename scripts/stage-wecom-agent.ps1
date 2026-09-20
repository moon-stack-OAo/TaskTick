# Stage self-contained wecom-agent.exe into src-tauri/binaries/ for Tauri externalBin.
# Usage (repo root):
#   pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/stage-wecom-agent.ps1
# Keep this file ASCII-only so Windows PowerShell 5.1 (CI default via npm) can parse it.

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$project = Join-Path $root "tools\wecom-agent\WecomAgent.csproj"
$publishDir = Join-Path $root "tools\wecom-agent\bin\Release\net10.0-windows\win-x64\publish"
$binariesDir = Join-Path $root "src-tauri\binaries"
$targetTriple = "x86_64-pc-windows-msvc"
$stagedName = "wecom-agent-$targetTriple.exe"
$stagedPath = Join-Path $binariesDir $stagedName

$dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
if (-not $dotnet) {
  $fallback = "C:\Program Files\dotnet\dotnet.exe"
  if (Test-Path -LiteralPath $fallback) {
    $dotnetExe = $fallback
  } else {
    throw "dotnet not found. Install .NET 10 SDK: https://dotnet.microsoft.com/download"
  }
} else {
  $dotnetExe = $dotnet.Source
}

Write-Host "dotnet: $dotnetExe"
Write-Host "publish -> $publishDir"

& $dotnetExe publish $project `
  -c Release `
  -r win-x64 `
  --self-contained true `
  -p:PublishSingleFile=true `
  -p:IncludeNativeLibrariesForSelfExtract=true `
  -p:EnableCompressionInSingleFile=true `
  -o $publishDir

if ($LASTEXITCODE -ne 0) {
  throw "dotnet publish failed, exit=$LASTEXITCODE"
}

$srcExe = Join-Path $publishDir "wecom-agent.exe"
if (-not (Test-Path -LiteralPath $srcExe)) {
  throw "wecom-agent.exe not generated: $srcExe"
}

New-Item -ItemType Directory -Force -Path $binariesDir | Out-Null
Copy-Item -LiteralPath $srcExe -Destination $stagedPath -Force

$len = (Get-Item -LiteralPath $stagedPath).Length
$mb = [math]::Round($len / 1MB, 1)
Write-Host "staged: $stagedPath ($mb MB)"
