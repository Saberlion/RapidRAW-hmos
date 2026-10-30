# build-ohos.ps1 — RapidRAW OpenHarmony HAP build (in-repo, self-contained; Windows host)
#
# Lives at scripts/build-ohos.ps1 so it survives TEMP cleanup; repo root is derived
# from this script's location. Machine-local prerequisites still apply (porting doc
# docs/HARMONYOS_PORTING.md section 8): DevEco Studio, ohos-devstudio junctions,
# ~/.cargo/bin clang wrappers, cargo-tauri (feat/open-harmony fork), ohrs,
# and the ONNX Runtime source submodule (git submodule update --init --recursive).
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
  [switch]$SkipBuild,            # skip the cargo stage; only inject ORT + report (HAP must already exist)
  [switch]$SkipOrtBuild,         # do not build ONNX Runtime from source (use an existing libs/ohos/<abi>/libonnxruntime.so)
  [switch]$RebuildOrt            # force a clean ONNX Runtime source build
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
if (-not $SkipOrtBuild -and -not (Test-Path "$repo\src-tauri\vendor\onnxruntime\cmake\CMakeLists.txt")) {
  Fail "ONNX Runtime source submodule missing - run: git submodule update --init --recursive"
}
# env.rs derives DEVECO_SDK_HOME as parent3(OHOS_HOME) — verify that exact computation
$p3 = Split-Path (Split-Path (Split-Path $env:OHOS_HOME -Parent) -Parent) -Parent
if (-not (Test-Path "$p3\default\sdk-pkg.json")) { Fail "parent3(OHOS_HOME) = $p3 is not a valid SDK root — env.rs DEVECO_SDK_HOME derivation will break (junction ohos-devstudio\default missing?)" }
Write-Host "preflight OK"

# ---------- 2.5 ONNX Runtime from source (pinned submodule) ----------
$targetAbi = @{ 'aarch64' = 'arm64-v8a'; 'armv7' = 'armeabi-v7a'; 'x86_64' = 'x86_64' }[$Target]
if (-not $SkipOrtBuild) {
  $ortSo = "$repo\src-tauri\libs\ohos\$targetAbi\libonnxruntime.so"
  $ortStamp = "$repo\src-tauri\libs\ohos\$targetAbi\onnxruntime-source.txt"
  # The stamp marks a library produced by build-ort-ohos.ps1. A .so without it
  # is a leftover prebuilt binary, which this branch no longer supports.
  if ($RebuildOrt -or -not (Test-Path $ortSo) -or -not (Test-Path $ortStamp)) {
    $ortCmd = 'powershell -NoProfile -ExecutionPolicy Bypass -File "' + "$repo\scripts\build-ort-ohos.ps1" + '" -Target ' + $Target
    if ($RebuildOrt) { $ortCmd += ' -Clean' }
    Invoke-Stage -Name 'ort-build' -Cmd $ortCmd -StageTimeoutSec 5400
  } else {
    Write-Host "using existing source-built ONNX Runtime: $ortSo (pass -RebuildOrt to rebuild)"
  }
}

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

# ---------- 3.5 inject ORT into the HAP (target ABI only) ----------
# ohrs build WIPES entry/libs/<abi>/ before copying the cargo dylib, so a .so placed
# there never survives into the HAP. The only channel is post-package zip injection.
# ORT is built from the pinned source submodule by section 2.5 and installed at
# src-tauri/libs/ohos/<abi>/libonnxruntime.so.
# Only the build target's ABI gets ORT; foreign ABI dirs (stale .so left behind by
# earlier builds of other targets, e.g. x86_64 emulator libs inside an aarch64
# package) are purged so store submissions ship clean and single-ABI.
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
  # AGC "package contains debug information" rejection (2026-10-11): hvigor's
  # ArkTS build packages ets/sourceMaps.map (EntryAbility sourcemap) into the
  # HAP. Store submissions must not carry it - delete the entry when present
  # (release buildMode usually omits it; this is the deterministic guarantee).
  # Note: the source-built libonnxruntime.so under src-tauri/libs/ohos/<abi>/
  # must also be stripped (llvm-strip --strip-unneeded) - AGC scans .so files
  # for DWARF sections; a backup of the unstripped original sits beside it.
  $srcMap = $zip.GetEntry('ets/sourceMaps.map')
  if ($srcMap) {
    $srcMap.Delete()
    Write-Host "removed ets/sourceMaps.map from HAP (AGC debug-info rule)"
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
    Write-Host "no source-built libonnxruntime.so under src-tauri/libs/ohos/$targetAbi - HAP ships without ORT (AI degrades gracefully)"
  }
  # CANN vendor libs (machine-local, same provisioning channel as ORT).
  # The HCL facade AND libai_fmk_tensor.so ride in the HAP, each with its
  # full transitive dependency closure. The facade is the one CANN piece
  # an app process cannot reach by name: /vendor/lib64/passthrough/ is
  # outside every searchable namespace, and files dropped into the app
  # lib dir post-install fail OHOS XPM code-signing (mmap EACCES) - so it
  # must ride inside the HAP. libai_fmk_tensor.so is nominally reachable
  # via /vendor/lib64/passthrough/indirect/, but the facade lazily
  # dlopens it by soname from moduleNs_default (app context) at
  # NDTensorBuffer creation, and that dlopen only succeeds when the
  # soname is already in moduleNs_default's loaded list or dirs - a copy
  # pre-loaded into the rich system namespace (where a bare-soname dlopen
  # of an indirect/ resident lands first) gets REJECTED with errno=2
  # (device-verified) - so tensor must load from the app lib dir too.
  # App-lib-dir loads land in moduleNs_default, whose dependency search
  # covers the app lib dir only - NOT the chipset-*/platformsdk subdirs
  # where this ROM keeps libhilog/libsec_shared etc., and not even
  # /system/lib64 root (the platform libc++.so symlink fails there with
  # errno=2; only loader-provided libc.so resolves) - so both closures
  # run down to leaf system libs, C++ runtime included (device-verified
  # level by level). Everything else - core, the _impl chain and the
  # whole vendor closure - is pulled into the rich default ns by
  # load_api's absolute-path dlopens from indirect/, where chipset
  # dirs ARE searchable (device-verified: libnpu_ai_fmk_om/libgraph/
  # libai_fmk_base all load with zero packing; the _impl chain must
  # NOT be packed, an app-dir copy would load it into moduleNs_default
  # and break it instead).
  #
  # Two namespace rules learned the hard way (T6/T7 device evidence):
  # - DT_NEEDED deps of app-dir (moduleNs_default) libs may be
  #   satisfied cross-namespace by copies the system stack already
  #   loaded in the default ns (T7: packed hilog/ffrt/ubsan/etc. never
  #   got mapped - the platformsdk/ndk copies satisfied them).
  # - Lazy dlopen() issued from moduleNs_default code resolves against
  #   ndk dirs + the app dir ONLY (no rich fallback), and a copy of the
  #   same soname already loaded in the default ns is cross-ns REJECTED
  #   instead of returned - so every soname the CANN client stack
  #   lazily dlopens MUST be packed here: libai_fmk_tensor.so (facade's
  #   buffer handle fetch, T6) and libai_hdi_request_client.so (tensor
  #   buffer creation's HDI ION allocation path, T7 - the dlopen failed
  #   with the lib sitting in indirect/ AND already mapped in the
  #   default ns through core's dependency closure).
  $cannLibs = @(
    'libai_fmk_hcl_model_runtime.so',
    'libai_infra_log.so',
    'libsec_shared.z.so',
    'libhitrace_meter.so',
    'libbegetutil.z.so',
    'libhilog.so',
    'libhitracechain.so',
    'libconfigpolicy_util.z.so',
    'libsystemparam.z.so',
    'libhilog_inner.so',
    'libhilog_encode.so',
    'libsandboxlog.so',
    'libcjson.z.so',
    'libclang_rt.ubsan_minimal.so',
    'libc++.so',
    'libai_fmk_tensor.so',
    'libai_infra_client.so',
    'libai_infra_dma_memory.so',
    'libai_infra_memory_manager_client.so',
    'libai_infra_rpc_client.so',
    'libai_fmk_tensor_cache_status_manager.so',
    'libai_hdi_request_client.so',
    'libdmabufheap.z.so',
    'libaccesstoken_compat_sdk.z.so',
    'libipc_single.z.so',
    'libbacktrace_local.so',
    'libfaultloggerd.z.so',
    'libunwinder.z.so',
    'libhisysevent.z.so',
    'libffrt.so',
    'libutils.z.so',
    'libdfx_util.z.so',
    'libdfx_procinfo.z.so',
    'liblzma.z.so',
    'libsamgr_proxy.z.so',
    'libipc_common.z.so',
    'libselinux.z.so',
    'libpcre2.z.so'
  )
  foreach ($cannLib in $cannLibs) {
    $cannSrc = "$repo\src-tauri\libs\ohos\$targetAbi\$cannLib"
    if (Test-Path $cannSrc) {
      $old = $zip.GetEntry("libs/$targetAbi/$cannLib")
      if ($old) { $old.Delete() }
      [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
        $zip, $cannSrc, "libs/$targetAbi/$cannLib",
        [IO.Compression.CompressionLevel]::Optimal) | Out-Null
      Write-Host "CANN lib injected into HAP libs/$targetAbi/: $cannLib"
    }
  }
} finally { $zip.Dispose() }

# ---------- 4. report artifact ----------
Write-Host "HAP: $($hap.FullName)"
Write-Host ("size: {0:N1} MB   built: {1}" -f ($hap.Length/1MB), $hap.LastWriteTime)
exit 0
