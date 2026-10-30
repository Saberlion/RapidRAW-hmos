// OHOS compatibility shims for ONNX Runtime's POSIX platform layer.
//
// OpenHarmony's musl libc exports sched_{get,set}affinity but does not provide
// the glibc-specific pthread_setaffinity_np() convenience wrapper, which
// onnxruntime/core/platform/posix/env.cc calls on every non-Android Linux
// (OHOS reports __linux__). ORT only logs a failure and continues, so mapping
// it to sched_setaffinity(0, ...) preserves the affinity request.
#ifndef ORT_OHOS_COMPAT_H
#define ORT_OHOS_COMPAT_H

#if defined(__OHOS__) && defined(__linux__)
#include <pthread.h>
#include <sched.h>
#include <stddef.h>

// Some ONNX Runtime MLAS translation units force `-march=armv8.2-a+dotprod`,
// which drops the BF16 target feature that arm_neon.h uses to gate its
// bfloat16_t typedef. MLAS declares bfloat16_t unconditionally on aarch64
// Linux, so provide an opaque placeholder when the feature is off (those TUs
// only take bfloat16_t pointers; the BF16 kernels build with +bf16).
#if defined(__aarch64__) && !defined(__ARM_FEATURE_BF16)
typedef struct bfloat16_t bfloat16_t;
#endif

#ifdef __cplusplus
extern "C" {
#endif

static inline int pthread_setaffinity_np(pthread_t thread, size_t cpusetsize,
                                         const cpu_set_t* cpuset) {
  (void)thread;  // sched_setaffinity(0, ...) addresses the calling thread
  return sched_setaffinity(0, cpusetsize, cpuset);
}

#ifdef __cplusplus
}
#endif

#endif  // __OHOS__ && __linux__

#endif  // ORT_OHOS_COMPAT_H
