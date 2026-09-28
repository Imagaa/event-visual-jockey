# Installs what the `ffmpeg` feature needs: LLVM (libclang for bindgen) + FFmpeg 8.1 LGPL shared dev build.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
if (-not (Test-Path 'C:\Program Files\LLVM\bin\libclang.dll')) {
    winget install --id LLVM.LLVM -e --silent --accept-source-agreements --accept-package-agreements
}
$dst = Join-Path $root 'third_party\ffmpeg'
if (-not (Test-Path "$dst\include")) {
    $zip = Join-Path $env:TEMP 'ffmpeg-dev.zip'
    Invoke-WebRequest 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n8.1-latest-win64-lgpl-shared-8.1.zip' -OutFile $zip
    Expand-Archive $zip -DestinationPath (Join-Path $root 'third_party') -Force
    Move-Item (Get-ChildItem (Join-Path $root 'third_party') -Directory -Filter 'ffmpeg-n8.1*').FullName $dst
}
Write-Host "Set for this shell:  `$env:FFMPEG_DIR='$dst'; `$env:LIBCLANG_PATH='C:\Program Files\LLVM\bin'; `$env:PATH+=';$dst\bin'"
