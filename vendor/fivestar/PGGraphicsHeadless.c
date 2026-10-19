// Headless graphics stub: links the vendored engine core on Android before the
// Vulkan renderer exists.
//
// This is deliberately dumb. The engine's graphics worker is started by
// w2c_env_wgpu_start_worker -> PGStartGraphics. That worker fills a D3D11-style
// command ring and blocks in PGGraphicsWait until a renderer acknowledges it.
// The stub does not acknowledge, so the graphics thread parks - which is the
// honest "no renderer" behaviour. Everything the engine owns that is NOT the
// graphics worker (boot, files, streaming, audio ring, input block, pthreads)
// still runs, which is what we are validating.
//
// This is not a renderer. Replace with libgta5vk.so when it lands and this file
// goes away; nothing above the graphics seam changes.
//
// ponytail: a parked graphics thread means the engine will eventually block the
// main thread waiting on the first present. That is expected and is exactly the
// signal that we have reached the renderer boundary. Remove once Vulkan is real.
#include "PGGraphicsAndroid.h"
#include <android/native_window.h>
#include <dlfcn.h>
#include <stdatomic.h>

static _Atomic uint64_t presented;
static ANativeWindow *window;

// Defined by EngineHost.c.
void pg_log(const char *message);

void PGAttachVulkanWindow(void *nativeWindow) {
  ANativeWindow *next = (ANativeWindow *)nativeWindow;
  if (window == next) return;
  if (window) {
    ANativeWindow_release(window);
    window = NULL;
  }
  window = next;
  if (window) ANativeWindow_acquire(window);
  atomic_store(&presented, 0);
}

void PGStartGraphics(void *memory, uint64_t memorySize, uint64_t ring, uint64_t ringSize, uint64_t debug) {
  (void)memory; (void)memorySize; (void)ring; (void)ringSize; (void)debug;
  pg_log("Headless graphics stub: command ring accepted but not rendered (Vulkan port pending).");
  // Deliberately no PGGraphicsWait acknowledgement: the graphics worker parks
  // here until a real renderer drains the ring.
}

double PGRenderedFramesPerSecond(void) {
  // The real renderer lives in the Rust library; ask it when it is there.
  typedef double (*pg_gpu_fps_fn)(void);
  pg_gpu_fps_fn fps=(pg_gpu_fps_fn)dlsym(RTLD_DEFAULT,"muguet_gpu_fps");
  return fps?fps():0.0;
}

void PGCaptureNativeFrame(const char *name) {
  (void)name;
  pg_log("Headless stub cannot capture a frame (no renderer).");
}