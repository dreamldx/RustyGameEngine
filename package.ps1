<#
.SYNOPSIS
    Builds a release binary and bundles it with the assets folder for distribution.

    Bevy resolves its asset root from the CARGO_MANIFEST_DIR environment variable
    when present (set by `cargo run`/`cargo build`), and falls back to the
    executable's own directory otherwise (see bevy_asset::io::file::get_base_path).
    A shipped exe run outside of cargo has no CARGO_MANIFEST_DIR, so it needs an
    "assets" folder next to it.
#>
param(
    [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"

cargo build --release

$exeName = "MarioEngine.exe"
$distDir = Join-Path $OutDir "MarioEngine"

if (Test-Path $distDir) {
    Remove-Item -Recurse -Force $distDir
}
New-Item -ItemType Directory -Force -Path $distDir | Out-Null

Copy-Item (Join-Path "target/release" $exeName) $distDir
Copy-Item -Recurse "assets" (Join-Path $distDir "assets")

Write-Host "Packaged to $distDir"
