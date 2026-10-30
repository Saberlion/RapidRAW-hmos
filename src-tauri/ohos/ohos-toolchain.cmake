# OpenHarmony CMake cross toolchain for native C/C++ dependencies
# (aws-lc-sys & friends) when cross-compiling RapidRAW with cargo's
# *-unknown-linux-ohos targets.
#
# Architecture is selected via the OHOS_ARCH env var: aarch64 (default,
# physical devices) or x86_64 (DevEco emulator). Unset = aarch64, so every
# previously documented recipe keeps working unchanged.
#
# Set OHOS_NDK_HOME to either the OpenHarmony native SDK directory (the one
# that contains llvm/ and sysroot/ directly) or the SDK root that contains
# native/. The latter is what cargo-mobile2 exports as OHOS_NDK_HOME during
# `cargo tauri ohos build` (ohrs derives tools as <OHOS_NDK_HOME>/native/llvm).
# If unset, falls back to the DevEco Studio default install location. Consumed
# via the CMAKE_TOOLCHAIN_FILE env var (picked up by cmake-rs); see
# docs/HARMONYOS_PORTING.md "Phase 0" for the full cross-compilation recipe.
set(OHOS_TOOLCHAIN "$ENV{OHOS_NDK_HOME}")
if(NOT OHOS_TOOLCHAIN)
  set(OHOS_TOOLCHAIN "C:/Program Files/Huawei/DevEco Studio/sdk/default/openharmony/native")
endif()

# Normalize Windows backslashes to CMake-style forward slashes: ORT embeds
# CMAKE_CXX_FLAGS into onnxruntime_config.h as a C string literal, and a
# sysroot like C:\Users\... produces invalid escape sequences ("\U", "\d").
file(TO_CMAKE_PATH "${OHOS_TOOLCHAIN}" OHOS_TOOLCHAIN)

# Tolerate both OHOS_NDK_HOME conventions: NDK root (llvm/ sits directly
# inside) vs SDK root (llvm/ sits inside native/). Normalize to the NDK root
# so the compiler/sysroot paths below work either way.
if(NOT EXISTS "${OHOS_TOOLCHAIN}/llvm")
  set(OHOS_TOOLCHAIN "${OHOS_TOOLCHAIN}/native")
endif()

set(OHOS_ARCH "$ENV{OHOS_ARCH}")
if(NOT OHOS_ARCH)
  set(OHOS_ARCH aarch64)
endif()
if(OHOS_ARCH STREQUAL "aarch64")
  set(OHOS_LLVM_TRIPLE "aarch64-linux-ohos")
elseif(OHOS_ARCH STREQUAL "x86_64")
  set(OHOS_LLVM_TRIPLE "x86_64-linux-ohos")
elseif(OHOS_ARCH STREQUAL "armv7")
  set(OHOS_LLVM_TRIPLE "arm-linux-ohos")
else()
  message(FATAL_ERROR "ohos-toolchain.cmake: unsupported OHOS_ARCH '${OHOS_ARCH}' (expected aarch64|x86_64|armv7)")
endif()

set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR ${OHOS_ARCH})

if(CMAKE_HOST_WIN32)
  set(_exe ".exe")
else()
  set(_exe "")
endif()

set(CMAKE_C_COMPILER   "${OHOS_TOOLCHAIN}/llvm/bin/clang${_exe}")
set(CMAKE_CXX_COMPILER "${OHOS_TOOLCHAIN}/llvm/bin/clang++${_exe}")
set(CMAKE_ASM_COMPILER "${OHOS_TOOLCHAIN}/llvm/bin/clang${_exe}")
set(CMAKE_AR           "${OHOS_TOOLCHAIN}/llvm/bin/llvm-ar${_exe}")
set(CMAKE_RANLIB       "${OHOS_TOOLCHAIN}/llvm/bin/llvm-ranlib${_exe}")

# Let CMake drive the OHOS target explicitly for its internal probes.
set(CMAKE_C_COMPILER_TARGET   ${OHOS_LLVM_TRIPLE})
set(CMAKE_CXX_COMPILER_TARGET ${OHOS_LLVM_TRIPLE})
set(CMAKE_ASM_COMPILER_TARGET ${OHOS_LLVM_TRIPLE})

set(CMAKE_C_FLAGS_INIT   "--target=${OHOS_LLVM_TRIPLE} --sysroot=${OHOS_TOOLCHAIN}/sysroot -D__MUSL__")
set(CMAKE_CXX_FLAGS_INIT "--target=${OHOS_LLVM_TRIPLE} --sysroot=${OHOS_TOOLCHAIN}/sysroot -D__MUSL__")
set(CMAKE_ASM_FLAGS_INIT "--target=${OHOS_LLVM_TRIPLE} --sysroot=${OHOS_TOOLCHAIN}/sysroot")

# ONNX Runtime's MLAS headers assume the ARMv8.2 FP16/BF16 extensions are
# visible on aarch64 Linux (bfloat16_t comes from arm_neon.h only when the
# target feature is enabled). Enable them for the whole translation unit; only
# the dedicated BF16/FP16 kernels actually emit those instructions.
if(OHOS_ARCH STREQUAL "aarch64")
  string(APPEND CMAKE_C_FLAGS_INIT " -march=armv8.2-a+fp16+bf16")
  string(APPEND CMAKE_CXX_FLAGS_INIT " -march=armv8.2-a+fp16+bf16")
  string(APPEND CMAKE_ASM_FLAGS_INIT " -march=armv8.2-a+fp16+bf16")
endif()

# OpenHarmony lacks glibc's pthread_setaffinity_np (ONNX Runtime calls it on
# every non-Android Linux). The shim maps it to sched_setaffinity(0, ...).
if(EXISTS "${CMAKE_CURRENT_LIST_DIR}/ort-ohos-compat.h")
  string(APPEND CMAKE_CXX_FLAGS_INIT " -include ${CMAKE_CURRENT_LIST_DIR}/ort-ohos-compat.h")
endif()

# Cross builds cannot link+run host executables: probe with static libs instead.
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)

set(CMAKE_FIND_ROOT_PATH "${OHOS_TOOLCHAIN}/sysroot")
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
