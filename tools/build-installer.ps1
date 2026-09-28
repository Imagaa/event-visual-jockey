# Builds dist\EVJ-Setup-<version>.exe: release build with the FFmpeg fallback decoder, the PPTX/PDF
# importer, ffmpeg.exe (Convert to HAP) and the FFmpeg LGPL DLLs.
# Usage: powershell -ExecutionPolicy Bypass -File tools\build-installer.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$ff = Join-Path $root 'third_party\ffmpeg'
if (-not (Test-Path "$ff\bin\ffmpeg.exe")) { & "$PSScriptRoot\setup-ffmpeg-dev.ps1" }
$env:FFMPEG_DIR = $ff
$env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'

Push-Location $root
try {
    # Own target dir: this exe needs the FFmpeg DLLs beside it, the dev build in target\release does not.
    $build = 'target\installer\build'
    # cargo reports progress on stderr, which 'Stop' would turn into an error in Windows PowerShell 5.1.
    $ErrorActionPreference = 'Continue'
    cargo build --release --features evj-app/ffmpeg -p evj-app -p evj-present --target-dir $build 2>&1 | ForEach-Object { "$_" }
    $ErrorActionPreference = 'Stop'
    if ($LASTEXITCODE) { throw 'cargo build failed' }
    $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value

    $stage = 'target\installer\stage'
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item -ItemType Directory -Force "$stage\licenses", "$stage\docs", "$stage\effects" | Out-Null
    Copy-Item "$build\release\evj.exe", "$build\release\evj-import.exe" $stage
    Copy-Item "$ff\bin\*.dll", "$ff\bin\ffmpeg.exe" $stage
    Copy-Item "$ff\LICENSE.txt" "$stage\licenses\FFmpeg-LGPL.txt"
    Copy-Item docs\USER-GUIDE.md, docs\TESTING-CHECKLIST.md "$stage\docs"
    Copy-Item effects\README.md "$stage\effects\README.md" -ErrorAction SilentlyContinue

    $iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $iscc) { throw 'Inno Setup 6 not found: winget install JRSoftware.InnoSetup' }
    & $iscc "/DAppVersion=$version" /Q installer\evj.iss
    if ($LASTEXITCODE) { throw 'ISCC failed' }
    Get-Item "dist\EVJ-Setup-$version.exe"
} finally {
    Pop-Location
}
