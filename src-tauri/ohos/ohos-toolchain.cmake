# OpenHarmony (aarch64-linux-ohos) CMake cross toolchain for native C/C++
# dependencies (aws-lc-sys & friends) when cross-compiling RapidRAW with
# cargo's aarch64-unknown-linux-ohos target.
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

# Tolerate both OHOS_NDK_HOME conventions: NDK root (llvm/ sits directly
# inside) vs SDK root (llvm/ sits inside native/). Normalize to the NDK root
# so the compiler/sysroot paths below work either way.
if(NOT EXISTS "${OHOS_TOOLCHAIN}/llvm")
  set(OHOS_TOOLCHAIN "${OHOS_TOOLCHAIN}/native")
endif()

set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR aarch64)

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
set(CMAKE_C_COMPILER_TARGET   aarch64-linux-ohos)
set(CMAKE_CXX_COMPILER_TARGET aarch64-linux-ohos)
set(CMAKE_ASM_COMPILER_TARGET aarch64-linux-ohos)

set(CMAKE_C_FLAGS_INIT   "--target=aarch64-linux-ohos --sysroot=${OHOS_TOOLCHAIN}/sysroot -D__MUSL__")
set(CMAKE_CXX_FLAGS_INIT "--target=aarch64-linux-ohos --sysroot=${OHOS_TOOLCHAIN}/sysroot -D__MUSL__")
set(CMAKE_ASM_FLAGS_INIT "--target=aarch64-linux-ohos --sysroot=${OHOS_TOOLCHAIN}/sysroot")

# Cross builds cannot link+run host executables: probe with static libs instead.
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)

set(CMAKE_FIND_ROOT_PATH "${OHOS_TOOLCHAIN}/sysroot")
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
