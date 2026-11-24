# Measures real LLVM source-based line/function coverage for OpenPresenter Rust crates
# using built-in `rustc -C instrument-coverage`, `llvm-profdata`, and `llvm-cov`.
param(
    [string[]]$Packages = @("core-scene", "core-ipc", "core-library", "render-text", "media-engine")
)

$ErrorActionPreference = "Stop"
$cargo = "C:\Users\admin\.cargo\bin\cargo.exe"
$rustc = "C:\Users\admin\.cargo\bin\rustc.exe"
$sysroot = (& $rustc --print sysroot).Trim()
$llvmBin = Get-ChildItem -Path "$sysroot\lib\rustlib" -Recurse -Filter "llvm-cov.exe" | Select-Object -First 1 -ExpandProperty DirectoryName
$profdata = Join-Path $llvmBin "llvm-profdata.exe"
$llvmCov = Join-Path $llvmBin "llvm-cov.exe"

$covDir = Join-Path $PSScriptRoot "..\target\coverage"
New-Item -ItemType Directory -Force -Path $covDir | Out-Null
Get-ChildItem -Path $covDir -Filter "*.profraw" -ErrorAction SilentlyContinue | Remove-Item -Force
Get-ChildItem -Path $covDir -Filter "*.profdata" -ErrorAction SilentlyContinue | Remove-Item -Force

$env:RUSTFLAGS = "-C instrument-coverage"
$env:LLVM_PROFILE_FILE = Join-Path $covDir "op-%p-%m.profraw"

$pkgArgs = @()
foreach ($p in $Packages) {
    $pkgArgs += "-p"
    $pkgArgs += $p
}

& $cargo test --offline --target-dir (Join-Path $covDir "target") @pkgArgs | Out-Null
if ($LASTEXITCODE -ne 0) { throw "Coverage test run failed" }

$profraws = Get-ChildItem -Path $covDir -Filter "*.profraw" | Select-Object -ExpandProperty FullName
$merged = Join-Path $covDir "merged.profdata"
& $profdata merge -sparse @profraws -o $merged

# Collect compiled unit test binaries (works on both legacy deps/ and Cargo 1.100 build/*/out/)
$bins = Get-ChildItem -Path (Join-Path $covDir "target\debug") -Recurse -Filter "*.exe" |
    Where-Object { $_.Name -match "^(core_scene|core_ipc|core_library|render_text|media_engine|broadcast_io|show_control|integrations|plugin_sdk|plugin_host)-[0-9a-f]+\.exe$" } |
    Select-Object -ExpandProperty FullName

$objArgs = @()
foreach ($b in $bins) {
    $objArgs += "--object"
    $objArgs += $b
}

& $llvmCov report --use-color=false --instr-profile=$merged @objArgs --ignore-filename-regex="(\\.cargo|rustc|\\\\deps\\\\)"
