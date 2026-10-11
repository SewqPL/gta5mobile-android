#pragma once
// Android substitutions for the three Apple APIs the vendored engine core uses.
//
// The upstream code reaches for Jetsam headroom (os_proc_available_memory), the
// simulator build flag (TARGET_OS_SIMULATOR), and dyld for trap logs. Android has
// no Jetsam, so headroom comes from /proc/meminfo MemAvailable, which is the
// closest honest equivalent: it is what the kernel would let a new allocation
// take without swapping. It cannot see a future kill, so the reserve in
// pg_streaming_headroom_budget is doing the real work here.
//
// ponytail: MemAvailable is system-wide, not this process's allowance. Upgrade
// path if a device proves too tight: poll ActivityManager.getProcessMemoryInfo
// through JNI and feed the callback below.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <pthread.h>
#include <dlfcn.h>

#define TARGET_OS_SIMULATOR 0

// Reads MemAvailable from /proc/meminfo. Returns 0 when unavailable, which makes
// nativeProcessHeadroom() bail out and leaves the engine's own budget in charge.
static inline uint64_t pg_port_available_memory(void) {
  FILE *f = fopen("/proc/meminfo", "re");
  if (!f) return 0;
  char line[256];
  uint64_t available = 0;
  while (fgets(line, sizeof line, f)) {
    if (strncmp(line, "MemAvailable:", 13) != 0) continue;
    unsigned long long kb = 0;
    if (sscanf(line + 13, "%llu", &kb) == 1) available = (uint64_t)kb * 1024;
    break;
  }
  fclose(f);
  return available;
}

static inline uint64_t os_proc_available_memory(void) { return pg_port_available_memory(); }

// Absolute-timeout wait on a MONOTONIC condvar. Replaces Apple's
// pthread_cond_timedwait_relative_np, which bionic does not provide. Callers must
// have initialized the condvar through pg_port_cond_init.
static inline int pg_port_cond_timedwait_relative(pthread_cond_t *condition,
                                                 pthread_mutex_t *mutex,
                                                 long seconds, long nanoseconds) {
  struct timespec deadline;
  clock_gettime(CLOCK_MONOTONIC, &deadline);
  deadline.tv_sec += seconds;
  deadline.tv_nsec += nanoseconds;
  if (deadline.tv_nsec >= 1000000000) {
    deadline.tv_sec++;
    deadline.tv_nsec -= 1000000000;
  }
  return pthread_cond_timedwait(condition, mutex, &deadline);
}

// Every condvar in the futex queue must use CLOCK_MONOTONIC or the deadline above
// is meaningless against bionic's CLOCK_REALTIME default.
static inline int pg_port_cond_init(pthread_cond_t *condition) {
  pthread_condattr_t attributes;
  pthread_condattr_init(&attributes);
  pthread_condattr_setclock(&attributes, CLOCK_MONOTONIC);
  int result = pthread_cond_init(condition, &attributes);
  pthread_condattr_destroy(&attributes);
  return result;
}

// Base address of the native library, for trap logs. Android has no dyld, but
// dladdr resolves the engine's own entry point just as well for that purpose.
// Engine heap sizing.
//
// Upstream hardcodes 4095 MB virtual / 3328 MB initial shared heap, sized for an
// iPhone with the extended-address-space entitlement. Android has no equivalent
// entitlement: largeHeap is a hint and OEMs vary widely (roughly 512 MB-1 GB on a
// 6 GB phone, up to ~2 GB on 12 GB), so those constants can exceed what the
// process may commit and the engine is killed rather than reporting a clean
// out-of-memory.
//
// This reads the same style of override the core already uses for PG_MEMORY_GUARD
// and PG_ENGINE_CORES. The Java launcher sets this before PGStartEngine.
// Clamped to 1..8 GB, matching the Rust/wasmtime Android path's MUGUET_ANDROID_HEAP_GB.
static inline uint64_t pg_port_engine_heap_bytes(void) {
  const char *value = getenv("PG_ENGINE_HEAP_MB");
  unsigned long mb = 3328;  // upstream default when unset
  if (value && *value) {
    char *end;
    unsigned long parsed = strtoul(value, &end, 10);
    if (end != value && *end == '\0') mb = parsed;
  }
  if (mb < 1024) mb = 1024;
  if (mb > 8192) mb = 8192;
  return (uint64_t)mb * 1048576ull;
}

// Virtual capacity to reserve. Must be >= the initial heap, and leaves room for
// the guard region that PG_UNCHECKED_MEMORY relies on. Upstream reserves a bit
// more than it commits (4095 vs 3328); keep that ratio so growth headroom exists.
static inline uint64_t pg_port_engine_reserve_bytes(uint64_t heapBytes) {
  uint64_t reserve = heapBytes + (heapBytes / 4);  // +25% virtual headroom
  uint64_t ceiling = 8192ull * 1048576ull;
  if (reserve > ceiling) reserve = ceiling;
  return reserve;
}

// Base address of the native library, for trap logs. Android has no dyld, but
// dladdr resolves the engine's own entry point just as well for that purpose.
static inline void *pg_port_image_base(void) {
  Dl_info info;
  if (dladdr((void *)(uintptr_t)&pg_port_image_base, &info) && info.dli_fbase) {
    return info.dli_fbase;
  }
  return NULL;
}