# 薄壳：直接调用 scripts/generate-icon.mjs（共享同一份 SVG → PNG 逻辑）。
# 仅在 Windows 上需要 PowerShell 入口时使用；与 .mjs 版本保持完全一致，避免双份实现漂移。
$ErrorActionPreference = "Stop"
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
& node (Join-Path $scriptDir "generate-icon.mjs")
exit $LASTEXITCODE