param(
    [switch]$Debug
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    if (-not $Debug) {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
        $executable = Join-Path $projectRoot "target\release\theme-inject.exe"
        Start-Process -FilePath $executable -ArgumentList "--open-panel"
        Write-Host "Theme Inject release rebuilt and started: $executable"
        return
    }

    $marker = Join-Path $projectRoot "target\theme-inject-dev-slot.txt"
    $activeSlot = if (Test-Path $marker) { (Get-Content -Raw $marker).Trim() } else { "b" }
    $nextSlot = if ($activeSlot -eq "a") { "b" } else { "a" }
    $targetDir = Join-Path $projectRoot "target\theme-inject-dev-$nextSlot"
    & cargo build --target-dir $targetDir
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
    Set-Content -NoNewline -Path $marker -Value $nextSlot
    $executable = Join-Path $targetDir "debug\theme-inject.exe"
    Get-Process theme-inject -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Process -FilePath $executable -ArgumentList "--open-panel"
    Write-Host "Theme Inject rebuilt in dev slot $nextSlot and restarted: $executable"
}
finally {
    Pop-Location
}
