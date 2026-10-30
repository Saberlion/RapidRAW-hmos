# build-ort-ohos.ps1 - build ONNX Runtime from source for OpenHarmony
#
# Compiles the pinned ONNX Runtime source (git submodule
# src-tauri/vendor/onnxruntime, tag v1.22.0) with the OHOS NDK toolchain and
# installs the resulting shared library at
#   src-tauri/libs/ohos/<abi>/libonnxruntime.so
# which is the path build.rs and scripts/build-ohos.ps1 already consume.
#
# This replaces the old "drop a prebuilt libonnxruntime.so into libs/ohos"
# workflow: from now on the library is produced from source on the build host.
# scripts/build-ohos.ps1 runs this script automatically when the .so is
# missing (see -SkipOrtBuild to opt out).
#
# Requirements (same host as the HAP build): DevEco Studio SDK/NDK, CMake+Ninja
# from DevEco (or the VS CMake bundle), Python 3.10+ with the packages in
# src-tauri/vendor/onnxruntime/requirements.txt (the script bootstraps a venv
# under src-tauri/target/ when they are missing).
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\build-ort-ohos.ps1
#   ... -Target x86_64            # emulator
#   ... -Clean                    # wipe the CMake build dir first
#   ... -Jobs 8                   # parallel compile jobs
param(
  [string]$Target = 'aarch64',   # aarch64 | armv7 | x86_64
  [switch]$Clean,
  [int]$Jobs = 0,                # 0 = let Ninja decide
  [switch]$NoStrip,              # keep DWARF (debugging); release HAPs must strip
  [switch]$ConfigureOnly         # stop after CMake configure (environment validation)
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$ortDir = "$repo\src-tauri\vendor\onnxruntime"
$cmakeLists = "$ortDir\cmake\CMakeLists.txt"

function Fail([string]$msg) { Write-Host "!! $msg" -ForegroundColor Red; exit 1 }

if (-not (Test-Path $cmakeLists)) {
  Fail "ONNX Runtime sources missing at $ortDir - run: git submodule update --init --recursive"
}
if (-not (Test-Path "$ortDir\cmake\external\onnx\CMakeLists.txt")) {
  Fail "ONNX submodule not initialized - run: git -C src-tauri/vendor/onnxruntime submodule update --init --recursive"
}

$abiByTarget = @{ 'aarch64' = 'arm64-v8a'; 'armv7' = 'armeabi-v7a'; 'x86_64' = 'x86_64' }
$tripleByTarget = @{
  'aarch64' = 'aarch64-unknown-linux-ohos'
  'armv7'   = 'armv7-unknown-linux-ohos'
  'x86_64'  = 'x86_64-unknown-linux-ohos'
}
$abi = $abiByTarget[$Target]
$triple = $tripleByTarget[$Target]
if (-not $abi) { Fail "unknown -Target '$Target' (expected aarch64|armv7|x86_64)" }

# ---------- environment (mirrors scripts/build-ohos.ps1 section 1) ----------
$dev = 'C:\Program Files\Huawei\DevEco Studio'
$vscmake = 'C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\IDE\CommonExtensions\Microsoft\CMake'
$sdkCandidates = @(
  "$env:USERPROFILE\ohos-devstudio\sdk\default\openharmony",
  "$dev\sdk\default\openharmony"
)
$sdk = $sdkCandidates | Where-Object { Test-Path "$_\native\llvm\bin\clang.exe" } | Select-Object -First 1
if (-not $sdk) { Fail "OHOS SDK not found (checked: $($sdkCandidates -join ', '))" }
$env:OHOS_HOME = $sdk
$env:OHOS_NDK_HOME = "$sdk\native"
$env:DEVECO_SDK_HOME = Split-Path (Split-Path $sdk -Parent) -Parent
$env:CMAKE_TOOLCHAIN_FILE = "$repo\src-tauri\ohos\ohos-toolchain.cmake"
$env:CMAKE_GENERATOR = 'Ninja'
$env:OHOS_ARCH = $Target
$env:Path = "$vscmake\CMake\bin;$vscmake\Ninja;$sdk\native\build-tools\cmake\bin;$env:Path"

foreach ($tool in @('cmake', 'ninja')) {
  if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { Fail "$tool not on PATH (DevEco / VS CMake required)" }
}
if (-not (Test-Path "$env:OHOS_NDK_HOME\llvm\bin\clang++.exe")) { Fail "OHOS NDK clang++ missing under $env:OHOS_NDK_HOME" }
# cc-rs-style per-target vars are not needed here (plain CMake), but keep the
# wrappers referenced by the toolchain docs available for nested projects.
$ccWrapper = "$env:USERPROFILE\.cargo\bin\$triple-clang.cmd"
if (-not (Test-Path $ccWrapper)) {
  Write-Host "warning: $ccWrapper missing - CMake uses the NDK clang directly, so this is only needed for cargo stages"
}

# ---------- Python + ONNX Runtime build requirements ----------
$buildRoot = "$repo\src-tauri\target\ort-ohos-$Target"
$venv = "$repo\src-tauri\target\ort-python"
function Test-PythonDeps([string]$python) {
  $prev = $ErrorActionPreference
  $ErrorActionPreference = 'SilentlyContinue'
  & $python -c "import flatbuffers, numpy, packaging" 2>&1 | Out-Null
  $ok = ($LASTEXITCODE -eq 0)
  $ErrorActionPreference = $prev
  return $ok
}
$python = $env:ORT_PYTHON
if (-not $python) {
  $systemPython = (Get-Command python -ErrorAction SilentlyContinue).Source
  if (-not $systemPython) { Fail "Python 3.10+ not found; set ORT_PYTHON to a python.exe" }
  $python = $systemPython
}
if (-not (Test-PythonDeps $python)) {
  Write-Host "==> bootstrapping Python build deps in $venv"
  if (-not (Test-Path "$venv\Scripts\python.exe")) {
    & $python -m venv $venv
    if ($LASTEXITCODE -ne 0) { Fail "failed to create venv at $venv" }
  }
  $venvPython = "$venv\Scripts\python.exe"
  & $venvPython -m pip install --disable-pip-version-check -q -i https://pypi.tuna.tsinghua.edu.cn/simple -r "$ortDir\requirements.txt"
  if ($LASTEXITCODE -ne 0) { Fail "pip install of ONNX Runtime build requirements failed" }
  $python = $venvPython
}
Write-Host "using Python: $python"
# CMake's Python discovery: put the selected interpreter first.
$env:Path = "$(Split-Path $python -Parent);$env:Path"

if ($Clean -and (Test-Path $buildRoot)) {
  Write-Host "==> removing $buildRoot"
  Remove-Item -Recurse -Force $buildRoot
}

# ---------- Eigen (GitLab archive is behind a Cloudflare challenge) ----------
# ORT fetches Eigen from gitlab.com; that endpoint answers automated downloads
# with a JS challenge. Bootstrap the exact upstream commit from the official
# GitHub mirror and point FetchContent at it, so the configure step never
# touches gitlab.com.
$eigenCommit = '1d8b82b0740839c0de7f1242a3585e3390ff5f33'
$eigenSha256 = '5f672dd5d594f148b473701417fdcdc7c137704a9ea32bf1f020705a5954af6f'
$eigenDir = $env:ORT_EIGEN_SOURCE
if (-not $eigenDir) { $eigenDir = "$repo\src-tauri\target\ort-deps\eigen" }
if (-not (Test-Path "$eigenDir\Eigen\Core")) {
  $eigenZip = "$repo\src-tauri\target\ort-deps\eigen.tar.gz"
  New-Item -ItemType Directory -Force -Path (Split-Path $eigenZip) | Out-Null
  Write-Host "==> fetching Eigen $eigenCommit (GitHub mirror)"
  & curl.exe -sSL --retry 5 --retry-all-errors --max-time 600 -o $eigenZip "https://codeload.github.com/eigenteam/eigen-git-mirror/tar.gz/$eigenCommit"
  if ($LASTEXITCODE -ne 0) { Fail "Eigen download failed (set ORT_EIGEN_SOURCE to an existing Eigen checkout)" }
  $actual = (Get-FileHash $eigenZip -Algorithm SHA256).Hash.ToLower()
  if ($actual -ne $eigenSha256) { Fail "Eigen archive hash mismatch: $actual (expected $eigenSha256)" }
  if (Test-Path $eigenDir) { Remove-Item -Recurse -Force $eigenDir }
  New-Item -ItemType Directory -Force -Path $eigenDir | Out-Null
  & tar.exe -xzf $eigenZip -C $eigenDir --strip-components=1
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path "$eigenDir\Eigen\Core")) { Fail "Eigen extraction failed" }
}
Write-Host "using Eigen source: $eigenDir"

# ---------- CMake configure ----------
# Minimal, OHOS-friendly feature set: shared ONNX Runtime, CPU EP only
# (DirectML/CUDA/QNN/NNAPI/XNNPACK/... are desktop/Android-specific), no
# tests, no Python/Java/ObjC/Node bindings, no training. Everything else
# (protobuf, flatbuffers, Eigen, ...) is fetched by ORT's own deps.txt.
$cmakeArgs = @(
  '-S', "$ortDir\cmake",
  '-B', $buildRoot,
  '-G', 'Ninja',
  '-DCMAKE_BUILD_TYPE=Release',
  "-DCMAKE_TOOLCHAIN_FILE=$repo\src-tauri\ohos\ohos-toolchain.cmake",
  '-DCMAKE_POSITION_INDEPENDENT_CODE=ON',
  "-DFETCHCONTENT_SOURCE_DIR_EIGEN3=$eigenDir",
  '-Donnxruntime_CROSS_COMPILING=ON',
  '-Donnxruntime_BUILD_SHARED_LIB=ON',
  '-Donnxruntime_BUILD_UNIT_TESTS=OFF',
  '-Donnxruntime_BUILD_BENCHMARKS=OFF',
  '-Donnxruntime_BUILD_JAVA=OFF',
  '-Donnxruntime_BUILD_OBJC=OFF',
  '-Donnxruntime_BUILD_NODEJS=OFF',
  '-Donnxruntime_BUILD_CSHARP=OFF',
  '-Donnxruntime_ENABLE_PYTHON=OFF',
  '-Donnxruntime_ENABLE_TRAINING=OFF',
  '-Donnxruntime_ENABLE_TRAINING_OPS=OFF',
  '-Donnxruntime_ENABLE_TRAINING_APIS=OFF',
  '-Donnxruntime_ENABLE_CPUINFO=OFF',
  '-Donnxruntime_USE_OPENMP=OFF',
  '-Donnxruntime_USE_MPI=OFF',
  '-Donnxruntime_USE_DML=OFF',
  '-Donnxruntime_USE_CUDA=OFF',
  '-Donnxruntime_USE_TENSORRT=OFF',
  '-Donnxruntime_USE_OPENVINO=OFF',
  '-Donnxruntime_USE_QNN=OFF',
  '-Donnxruntime_USE_NNAPI_BUILTIN=OFF',
  '-Donnxruntime_USE_XNNPACK=OFF',
  '-Donnxruntime_USE_ARMNN=OFF',
  '-Donnxruntime_USE_ACL=OFF',
  '-Donnxruntime_USE_MIGRAPHX=OFF',
  '-Donnxruntime_USE_ROCM=OFF',
  '-Donnxruntime_USE_COREML=OFF',
  '-Donnxruntime_USE_WEBNN=OFF',
  '-Donnxruntime_USE_VITISAI=OFF',
  '-Donnxruntime_USE_RKNPU=OFF',
  '-Donnxruntime_DISABLE_CONTRIB_OPS=OFF'
)
Write-Host "==> configuring ONNX Runtime for OHOS/$Target"
& cmake @cmakeArgs
if ($LASTEXITCODE -ne 0) { Fail "cmake configure failed" }
if ($ConfigureOnly) {
  Write-Host "OK: configure-only run finished ($buildRoot)"
  exit 0
}

# ---------- build ----------
$buildArgs = @('--build', $buildRoot, '--target', 'onnxruntime')
if ($Jobs -gt 0) { $buildArgs += @('--parallel', "$Jobs") }
Write-Host "==> building onnxruntime (this takes a while on first run)"
& cmake @buildArgs
if ($LASTEXITCODE -ne 0) { Fail "cmake build failed" }

# ---------- install ----------
$so = Get-ChildItem -Path $buildRoot -Recurse -Filter 'libonnxruntime.so' -File -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $so) { Fail "libonnxruntime.so not produced under $buildRoot" }

$libsDir = "$repo\src-tauri\libs\ohos\$abi"
New-Item -ItemType Directory -Force -Path $libsDir | Out-Null
$dest = "$libsDir\libonnxruntime.so"
Copy-Item $so.FullName $dest -Force
Write-Host "built: $($so.FullName)"
Write-Host "installed: $dest"

if (-not $NoStrip) {
  $strip = "$env:OHOS_NDK_HOME\llvm\bin\llvm-strip.exe"
  if (Test-Path $strip) {
    Copy-Item $dest "$dest.unstripped" -Force
    & $strip --strip-unneeded $dest
    if ($LASTEXITCODE -ne 0) { Fail "llvm-strip failed (unstripped copy kept at $dest.unstripped)" }
    Write-Host "stripped: $dest (unstripped copy: $dest.unstripped)"
  } else {
    Write-Host "warning: llvm-strip not found at $strip - leave the library unstripped only for debug builds"
  }
}

Write-Host "OK: ONNX Runtime from source is ready at $dest"
$a = "v1.22.0-from-source"
Set-Content -Path "$libsDir\onnxruntime-source.txt" -Value $a -Encoding ASCII
Write-Host "stamp: $(Join-Path $libsDir 'onnxruntime-source.txt') ($a)"
