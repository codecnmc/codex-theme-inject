param(
    [switch]$Debug
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
$profile = if ($Debug) { "debug" } else { "release" }
$arguments = if ($Debug) { @("build") } else { @("build", "--release") }

Get-Process theme-inject -ErrorAction SilentlyContinue | Stop-Process -Force
Push-Location $projectRoot
try {
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
    $executable = Join-Path $projectRoot "target\$profile\theme-inject.exe"
    Start-Process -FilePath $executable -ArgumentList "--open-panel"
    Write-Host "Theme Inject rebuilt and restarted: $executable"
}
finally {
    Pop-Location
}
