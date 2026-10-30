# convert-nind-ms.ps1 — NIND denoise ONNX -> MindSpore Lite .ms (reproducible)
#
# Reproduces the canonical, device-verified nind_denoise_utnet_684.ms
# (SHA256 = DENOISE_MS_SHA256 in src-tauri/src/ai_processing.rs). Pipeline:
#
#   1. source ONNX    -OnnxPath, or auto-download from DENOISE_URL (the same
#                       URL the app uses at runtime), sha256-verified
#   2. graph surgery  scripts/nind-fix-pad.py: negative-pads Pad (crop
#                       semantics) -> equivalent Slice; MS Lite PadFusion
#                       rejects negative pads at runtime (InferShape failure)
#   3. conversion     converter_lite 2.7.0 via WSL. The converter version
#                       MUST be <= the device MindSpore Lite Kit engine
#                       (2.7.0 on the HAD-W32; a 2.10.0 .ms fails on device
#                       with "Wrong input size"). The Linux x64 package lives
#                       at ~/mslite inside WSL (-SetupWsl provisions it).
#   4. verify         output sha256 == DENOISE_MS_SHA256. Hard gate: any
#                       drift (onnx package version, converter version, model
#                       revision) fails loudly instead of shipping a divergent
#                       .ms.
#
# Determinism: full pipeline re-run 2026-10-08 reproduced byte-identical
# output (onnx 1.23.2 + mindspore-lite-2.7.0-linux-x64). Intermediate hashes
# are pinned below and checked at every stage.
#
# Prereqs: Windows host with WSL (default distro; bash/curl/tar), python with
# onnx + numpy on PATH. converter_lite ships for Windows/Linux x64 only (no
# macOS/Android/OHOS converter exists) — this script uses the WSL Linux path
# because that is the byte-exact toolchain the canonical .ms came from.
#
# Usage (from repo root):
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\convert-nind-ms.ps1
#   powershell ... -File scripts\convert-nind-ms.ps1 -OnnxPath C:\models\nind_denoise_utnet_684.onnx
#   powershell ... -File scripts\convert-nind-ms.ps1 -OutDir D:\out -SetupWsl
param(
  [string]$OnnxPath = '',                  # local source .onnx (skips download)
  [string]$OutDir = (Get-Location).Path,   # where nind_denoise_utnet_684.ms lands
  [switch]$SetupWsl                        # provision the 2.7.0 converter pkg in WSL if missing
)
$ErrorActionPreference = 'Stop'
$repo = (Split-Path -Parent $PSScriptRoot)

# ---------- pinned constants (MUST match src-tauri/src/ai_processing.rs) ----------
$DENOISE_URL    = 'https://hf-mirror.com/CyberTimon/RapidRAW-Models/resolve/main/nind_denoise_utnet_684.onnx?download=true'
$DENOISE_SHA256 = 'ee3586279d514df557ff3f7dec6df37fafc51ba5d3a3435b2cc9ac2d9017e7fe'
$SLICED_SHA256  = 'ad157c918cc87888de5c07dd7f93ebef7d93778a5ca369e561300a39720e155a'
$MS_FILENAME    = 'nind_denoise_utnet_684.ms'
$MS_SHA256      = '05d7580e5a6e3c18bd286705f7524090cd69abe4b78067c2a173b322ef151b3b'
$MSLITE_VER     = '2.7.0'
$MSLITE_URL     = "https://ms-release.obs.cn-north-4.myhuaweicloud.com/2.7.0/MindSporeLite/lite/release/linux/x86_64/mindspore-lite-$MSLITE_VER-linux-x64.tar.gz"

function Fail([string]$msg) { Write-Host "!! $msg" -ForegroundColor Red; exit 1 }
function Get-Sha256([string]$p) { (Get-FileHash $p -Algorithm SHA256).Hash.ToLower() }
function To-WslPath([string]$p) {
  # C:\a\b -> /mnt/c/a/b (drive letter lowercased only; Linux is case-sensitive)
  if ($p -match '^([A-Za-z]):\\(.*)$') { '/mnt/' + $Matches[1].ToLower() + '/' + ($Matches[2] -replace '\\', '/') }
  else { $p -replace '\\', '/' }
}

# ---------- 0. preflight ----------
python -c "import onnx, numpy" | Out-Null
if ($LASTEXITCODE -ne 0) { Fail "python with onnx+numpy not on PATH (determinism verified with onnx 1.23.2)" }
$wslProbe = wsl bash -lc 'echo WSL_OK'
if ($LASTEXITCODE -ne 0 -or "$wslProbe" -notmatch 'WSL_OK') { Fail "WSL not available (converter_lite runs on Linux x64 only)" }

# ---------- 1. source onnx ----------
$work = "$env:TEMP\opencode\convert-nind-ms"
New-Item -ItemType Directory -Path $work -Force | Out-Null
if (-not $OnnxPath) {
  $OnnxPath = "$work\nind_denoise_utnet_684.onnx"
  if (-not (Test-Path $OnnxPath) -or (Get-Sha256 $OnnxPath) -ne $DENOISE_SHA256) {
    Write-Host "==> downloading source ONNX from DENOISE_URL (~118 MB)"
    # PS 5.1 Invoke-WebRequest throws on hf-mirror's 308 Permanent Redirect;
    # curl.exe (bundled since Windows 10 1803) follows it fine.
    curl.exe -L --fail -o $OnnxPath $DENOISE_URL
    if ($LASTEXITCODE -ne 0) { Fail "download failed (curl exit $LASTEXITCODE)" }
  }
}
if (-not (Test-Path $OnnxPath)) { Fail "source onnx not found: $OnnxPath" }
$h = Get-Sha256 $OnnxPath
if ($h -ne $DENOISE_SHA256) { Fail "source onnx sha256 mismatch: $h (expected $DENOISE_SHA256)" }
Write-Host ("OK  source ONNX verified ({0:N1} MB)" -f ((Get-Item $OnnxPath).Length / 1MB))

# ---------- 2. graph surgery (negative-pads Pad -> Slice) ----------
$sliced = "$work\nind_sliced.onnx"
Write-Host "==> graph surgery (scripts/nind-fix-pad.py)"
python "$repo\scripts\nind-fix-pad.py" $OnnxPath $sliced | Out-Null
if ($LASTEXITCODE -ne 0) { Fail "graph surgery failed" }
$h = Get-Sha256 $sliced
if ($h -ne $SLICED_SHA256) { Fail "surgery output hash drifted: $h (expected $SLICED_SHA256) - onnx package version drift?" }
Write-Host "OK  surgery output matches pinned hash"

# ---------- 3. converter_lite 2.7.0 via WSL ----------
$pkg = "~/mslite/mindspore-lite-$MSLITE_VER-linux-x64"
$conv = "$pkg/tools/converter/converter/converter_lite"
$probe = wsl bash -lc "test -x $conv && echo CONV_OK"
if ("$probe" -notmatch 'CONV_OK') {
  if (-not $SetupWsl) { Fail "converter_lite $MSLITE_VER missing in WSL at $conv - re-run with -SetupWsl to provision (~66 MB from ms-release.obs.cn-north-4.myhuaweicloud.com)" }
  Write-Host "==> provisioning WSL converter package (~66 MB)"
  $prov = 'mkdir -p ~/mslite && cd ~/mslite && curl -sL -o mslite-' + $MSLITE_VER + '.tar.gz ' + $MSLITE_URL + ' && tar xzf mslite-' + $MSLITE_VER + '.tar.gz && test -x ' + $conv
  wsl bash -lc $prov
  if ($LASTEXITCODE -ne 0) { Fail "WSL converter provisioning failed" }
}
Write-Host "==> converting (converter_lite $MSLITE_VER via WSL)"
$log = "$work\convert.log"
$rawOut = "$work\nind_ms_raw"
$cmd = 'export LD_LIBRARY_PATH=' + $pkg + '/tools/converter/lib:$LD_LIBRARY_PATH; ' + $conv +
  ' --fmk=ONNX --modelFile=' + (To-WslPath $sliced) + ' --outputFile=' + (To-WslPath $rawOut)
wsl bash -lc $cmd | Out-File $log -Encoding utf8
if ($LASTEXITCODE -ne 0 -or -not (Test-Path "$rawOut.ms")) {
  Get-Content $log -Tail 8 -ErrorAction SilentlyContinue | Write-Host
  Fail "conversion failed (expect 'CONVERT RESULT SUCCESS:0')"
}
Get-Content $log -Tail 3 | Write-Host

# ---------- 4. verify + place ----------
$h = Get-Sha256 "$rawOut.ms"
if ($h -ne $MS_SHA256) { Fail "converted .ms hash drifted: $h (expected $MS_SHA256)" }
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
Copy-Item "$rawOut.ms" (Join-Path $OutDir $MS_FILENAME) -Force
Write-Host "OK  $MS_FILENAME -> $OutDir (sha256 verified)"
Write-Host "next: upload to the repo behind DENOISE_MS_URL, or sideload to a device:"
Write-Host "  hdc file send $OutDir\$MS_FILENAME /data/local/tmp/$MS_FILENAME"
Write-Host "  hdc shell cp /data/local/tmp/$MS_FILENAME /data/app/el2/100/base/io.github.CyberTimon.RapidRAW/files/appdata/models/"
exit 0
