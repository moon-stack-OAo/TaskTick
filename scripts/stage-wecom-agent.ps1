# 将 wecom-agent 发布为自包含单文件，并复制到 src-tauri/binaries/
# 供 Tauri bundle.externalBin 打入安装包（与 TaskTick.exe 同目录）。
# 用法（仓库根目录）:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/stage-wecom-agent.ps1

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
  if (Test-Path $fallback) {
    $dotnetExe = $fallback
  } else {
    throw "未找到 dotnet。请安装 .NET 10 SDK：https://dotnet.microsoft.com/download"
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
  throw "dotnet publish 失败，exit=$LASTEXITCODE"
}

$srcExe = Join-Path $publishDir "wecom-agent.exe"
if (-not (Test-Path $srcExe)) {
  throw "未生成 wecom-agent.exe: $srcExe"
}

New-Item -ItemType Directory -Force -Path $binariesDir | Out-Null
Copy-Item -LiteralPath $srcExe -Destination $stagedPath -Force

$len = (Get-Item -LiteralPath $stagedPath).Length
Write-Host ("staged: {0} ({1:N1} MB)" -f $stagedPath, ($len / 1MB))
