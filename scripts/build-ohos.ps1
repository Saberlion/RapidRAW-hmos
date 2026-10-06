# build-ohos.ps1 — RapidRAW OpenHarmony HAP build (in-repo, self-contained; Windows host)
#
# Lives at scripts/build-ohos.ps1 so it survives TEMP cleanup; repo root is derived
# from this script's location. Machine-local prerequisites still apply (porting doc
# docs/HARMONYOS_PORTING.md section 8): DevEco Studio, ohos-devstudio junctions,
# ~/.cargo/bin clang wrappers, cargo-tauri (feat/open-harmony fork), ohrs,
# and src-tauri/libs/ohos/<abi>/libonnxruntime.so (gitignored, per machine).
#
# Design goals:
#   1. FAIL FAST  — preflight checks stop before the long build if the environment is broken;
#                   the first failing stage aborts the script and prints the log tail.
#   2. NO HANGS   — every stage runs through a generated .cmd runner with FILE redirection
#                   (immune to the hvigor daemon pipe-holding hang, porting doc 6.3),
#                   under a watchdog that kills the whole process tree on timeout.
#                   A heartbeat prints log growth so a long build never looks stuck.
#
# Usage (from repo root):
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-ohos.ps1
#   powershell ... -File scripts\build-ohos.ps1 -TimeoutSec 3600
#   powershell ... -File scripts\build-ohos.ps1 -Target x86_64            # DevEco emulator
#   powershell ... -File scripts\build-ohos.ps1 -SkipBuild                # re-inject ORT only
#   powershell ... -File scripts\build-ohos.ps1 -Release                  # release profile (AppGallery submission)
#   powershell ... -File scripts\build-ohos.ps1 -ExtraArgs @('--','--features','tethering')  # passthrough
param(
  [int]$TimeoutSec = 1800,
  [string[]]$ExtraArgs = @(),
  [string]$Target = 'aarch64',   # aarch64 (devices) | x86_64 (DevEco emulator) | armv7
  [switch]$Release,              # release profile; default is debug (-d). ORT injection is always target-ABI-only and foreign ABI dirs are purged.
  [switch]$SkipBuild             # skip the cargo stage; only inject ORT + report (HAP must already exist)
)
$ErrorActionPreference = 'Stop'
$repo = (Split-Path -Parent $PSScriptRoot)
$logDir = "$env:TEMP\opencode"
$log = "$logDir\ohos-build-auto.log"
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

function Fail([string]$msg) { Write-Host "!! $msg" -ForegroundColor Red; exit 1 }

# ---------- stage runner: fail-fast + watchdog + heartbeat ----------
function Invoke-Stage {
  param([string]$Name, [string]$Cmd, [int]$StageTimeoutSec)
  $runner = "$logDir\stage-$Name.cmd"
  $body = "@echo off`r`ncd /d ""$repo""`r`n$Cmd > ""$log"" 2>&1`r`nexit /b %ERRORLEVEL%"
  Set-Content -Path $runner -Value $body -Encoding ASCII
  Write-Host "==> [$Name] started (watchdog ${StageTimeoutSec}s)"
  $p = Start-Process -FilePath $runner -PassThru -WindowStyle Hidden
  $deadline = (Get-Date).AddSeconds($StageTimeoutSec)
  $lastLen = -1
  while (-not $p.HasExited) {
    if ((Get-Date) -gt $deadline) {
      Write-Host "!! [$Name] TIMEOUT after ${StageTimeoutSec}s — killing process tree" -ForegroundColor Red
      taskkill /T /F /PID $p.Id 2>$null | Out-Null
      Get-Content $log -Tail 30 -ErrorAction SilentlyContinue | Write-Host
      exit 124
    }
    Start-Sleep -Seconds 10
    $len = (Get-Item $log -ErrorAction SilentlyContinue).Length
    if ($len -and $len -ne $lastLen) { Write-Host "    [$Name] ... in progress, log $([math]::Round($len/1KB)) KB"; $lastLen = $len }
  }
  if ($p.ExitCode -ne 0) {
    Write-Host "!! [$Name] FAILED exit=$($p.ExitCode) — last 45 log lines:" -ForegroundColor Red
    Get-Content $log -Tail 45 -ErrorAction SilentlyContinue | Write-Host
    exit $p.ExitCode
  }
  Write-Host "OK  [$Name]"
}

# ---------- 0. hygiene: kill stale hvigor daemons (porting doc 6.3) ----------
Get-CimInstance Win32_Process -Filter "name='node.exe' or name='java.exe'" -ErrorAction SilentlyContinue |
  Where-Object { $_.CommandLine -match 'hvigor' } |
  ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue; Write-Host "killed stale hvigor daemon PID $($_.ProcessId)" }

# ---------- 1. environment (self-contained; mirrors porting doc section 8) ----------
$dev = 'C:\Program Files\Huawei\DevEco Studio'
$vscmake = 'C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\IDE\CommonExtensions\Microsoft\CMake'
$env:OHOS_HOME = "$env:USERPROFILE\ohos-devstudio\sdk\default\openharmony"
$env:OHOS_NDK_HOME = "$env:USERPROFILE\ohos-devstudio\sdk\default\openharmony\native"
$env:DEVECO_SDK_HOME = "$env:USERPROFILE\ohos-devstudio\sdk"
$env:JAVA_HOME = "$dev\jbr"
$env:Path = "$vscmake\CMake\bin;$vscmake\Ninja;$dev\tools\ohpm\bin;$dev\tools\hvigor\bin;$dev\jbr\bin;$env:USERPROFILE\.cargo\bin;C:\Program Files\nodejs;" + $env:Path
$env:CMAKE_TOOLCHAIN_FILE = "$repo\src-tauri\ohos\ohos-toolchain.cmake"
$env:CMAKE_GENERATOR = 'Ninja'
$env:ORT_SKIP_DOWNLOAD = '1'
$env:ORT_DYLIB_PATH = 'libonnxruntime.so'
# cc-rs per-target env vars — map CLI short target name to the full rust triple
$_triple = @{ 'aarch64' = 'aarch64-unknown-linux-ohos'; 'armv7' = 'armv7-unknown-linux-ohos'; 'x86_64' = 'x86_64-unknown-linux-ohos' }[$Target]
if (-not $_triple) { Fail "unknown -Target '$Target' (expected aarch64|armv7|x86_64)" }
$_t = $_triple -replace '-', '_'
Set-Item -Path "Env:CC_$_t"  -Value "$env:USERPROFILE\.cargo\bin\$_triple-clang.cmd"
Set-Item -Path "Env:CXX_$_t" -Value "$env:USERPROFILE\.cargo\bin\$_triple-clang++.cmd"
Set-Item -Path "Env:AR_$_t"  -Value "$env:USERPROFILE\.cargo\bin\$_triple-ar.cmd"
# ohos-toolchain.cmake arch selector (defaults to aarch64 when unset)
$env:OHOS_ARCH = $Target
$env:RUSTUP_DIST_SERVER = 'https://mirrors.ustc.edu.cn/rust-static'
$env:NO_COLOR = '1'

# ---------- 2. preflight: fail in seconds, not after a 20-minute build ----------
if (-not (Test-Path "$repo\src-tauri\tauri.conf.json")) { Fail "repo path broken: $repo" }
if (-not (Test-Path "$repo\src-tauri\ohos\ohos-toolchain.cmake")) { Fail "missing ohos-toolchain.cmake" }
if (-not (Test-Path "$repo\node_modules")) { Fail "node_modules missing — run 'npm.cmd install' at repo root first" }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Fail "cargo not on PATH (rustup broken?)" }
if (-not (Test-Path "$env:USERPROFILE\.cargo\bin\cargo-tauri.exe")) { Fail "cargo-tauri (feat/open-harmony fork) not installed" }
if (-not (Test-Path "$env:USERPROFILE\.cargo\bin\ohrs.exe")) { Fail "ohrs not installed" }
if (-not (Get-Command java -ErrorAction SilentlyContinue)) { Fail "java not on PATH — PackageHap needs it (DevEco jbr)" }
if (-not (Get-Command node -ErrorAction SilentlyContinue)) { Fail "node not on PATH" }
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) { Fail "cmake not on PATH" }
if (-not (Get-Command ninja -ErrorAction SilentlyContinue)) { Fail "ninja not on PATH" }
if (-not (Test-Path "$env:OHOS_NDK_HOME\llvm\bin\clang.exe")) { Fail "OHOS NDK clang unreachable via $env:OHOS_NDK_HOME (junction broken?)" }
if (-not (Test-Path "$env:USERPROFILE\.cargo\bin\$_triple-clang.cmd")) { Fail "missing wrapper $_triple-clang.cmd in ~/.cargo/bin (see docs 6.1)" }
if (-not (Test-Path "$env:DEVECO_SDK_HOME\default\sdk-pkg.json")) { Fail "DEVECO_SDK_HOME has no default\sdk-pkg.json — hvigor will hit 00303312" }
# env.rs derives DEVECO_SDK_HOME as parent3(OHOS_HOME) — verify that exact computation
$p3 = Split-Path (Split-Path (Split-Path $env:OHOS_HOME -Parent) -Parent) -Parent
if (-not (Test-Path "$p3\default\sdk-pkg.json")) { Fail "parent3(OHOS_HOME) = $p3 is not a valid SDK root — env.rs DEVECO_SDK_HOME derivation will break (junction ohos-devstudio\default missing?)" }
Write-Host "preflight OK"

# ---------- 3. build (single cargo stage; cargo/ohrs/hvigor failures all surface via exit code) ----------
if (-not $SkipBuild) {
  # Release needs the runner passthrough: the tauri CLI's own cargo stage
  # follows -d/--debug, but the ohrs stage (which places the .so that hvigor
  # packages) defaults to dev unless it receives --release via `--` args.
  if ($Release) {
    $buildArgs = (@('cargo', 'tauri', 'ohos', 'build', '-t', $Target, '--', '--release') + $ExtraArgs) -join ' '
  } else {
    $buildArgs = (@('cargo', 'tauri', 'ohos', 'build', '-d', '-t', $Target) + $ExtraArgs) -join ' '
  }
  Invoke-Stage -Name 'ohos-build' -Cmd $buildArgs -StageTimeoutSec $TimeoutSec
}

# ---------- 3.5 inject prebuilt ORT into the HAP (target ABI only) ----------
# ohrs build WIPES entry/libs/<abi>/ before copying the cargo dylib, so a .so placed
# there never survives into the HAP. The only channel is post-package zip injection.
# Machine-local ORT lives at src-tauri/libs/ohos/<abi>/libonnxruntime.so (gitignored).
# Only the build target's ABI gets ORT; foreign ABI dirs (stale .so left behind by
# earlier builds of other targets, e.g. x86_64 emulator libs inside an aarch64
# package) are purged so store submissions ship clean and single-ABI.
$targetAbi = @{ 'aarch64' = 'arm64-v8a'; 'armv7' = 'armeabi-v7a'; 'x86_64' = 'x86_64' }[$Target]
# hvigor emits *-unsigned.hap when no signingConfig applies, or *-signed.hap when the
# build-profile signingConfig makes SignHap run. Entry content is identical either way
# (the signing block lives outside the zip entries). The ORT injection below rewrites
# the zip via .NET Update mode, which drops any signing block — pack-app.ps1 re-signs
# afterwards, so both names are valid injection targets. Debug-signed leftovers are
# excluded by name.
$hap = Get-ChildItem -Path "$repo\src-tauri\gen\ohos\entry\build\*\outputs\*\*.hap" -File -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -notmatch 'debug' } |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $hap) { Fail "no *.hap found under gen/ohos/entry/build (build first, or drop -SkipBuild)" }

Add-Type -AssemblyName System.IO.Compression.FileSystem
# HAP abi -> OHOS NDK triple (for libc++_shared.so, the runtime dependency of libonnxruntime.so)
$ndkLibcxx = @{
  'arm64-v8a'   = "$env:OHOS_NDK_HOME\llvm\lib\aarch64-linux-ohos\libc++_shared.so"
  'armeabi-v7a' = "$env:OHOS_NDK_HOME\llvm\lib\armv7-linux-ohos\libc++_shared.so"
  'x86_64'      = "$env:OHOS_NDK_HOME\llvm\lib\x86_64-linux-ohos\libc++_shared.so"
}
$zip = [IO.Compression.ZipFile]::Open($hap.FullName, 'Update')
try {
  $foreign = @($zip.Entries | Where-Object { $_.FullName -like 'libs/*' -and ($_.FullName.Split('/')[1]) -ne $targetAbi })
  if ($foreign) {
    $foreign | ForEach-Object { $_.Delete() }
    Write-Host "purged foreign ABI entries from HAP libs/: $(($foreign | ForEach-Object { $_.FullName }) -join ', ')"
  }
  $ortSrc = "$repo\src-tauri\libs\ohos\$targetAbi\libonnxruntime.so"
  if (Test-Path $ortSrc) {
    $old = $zip.GetEntry("libs/$targetAbi/libonnxruntime.so")
    if ($old) { $old.Delete() }
    [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
      $zip, $ortSrc, "libs/$targetAbi/libonnxruntime.so",
      [IO.Compression.CompressionLevel]::Optimal) | Out-Null
    # libonnxruntime.so needs libc++_shared.so; cmake only ships it for the
    # arch it built (arm64-v8a), so supply the NDK one when the HAP lacks it.
    if (-not $zip.GetEntry("libs/$targetAbi/libc++_shared.so") -and (Test-Path $ndkLibcxx[$targetAbi])) {
      [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
        $zip, $ndkLibcxx[$targetAbi], "libs/$targetAbi/libc++_shared.so",
        [IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
    Write-Host "ORT injected into HAP libs/$targetAbi/"
  } else {
    Write-Host "no machine-local libonnxruntime.so under src-tauri/libs/ohos/$targetAbi - HAP ships without ORT (AI degrades gracefully)"
  }
} finally { $zip.Dispose() }

# ---------- 4. report artifact ----------
Write-Host "HAP: $($hap.FullName)"
Write-Host ("size: {0:N1} MB   built: {1}" -f ($hap.Length/1MB), $hap.LastWriteTime)
exit 0
