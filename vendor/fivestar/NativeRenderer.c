// The renderer is the Rust library (libmuguet.so), the same one the wasmtime
// path draws with. The engine core never links against it: it is resolved at
// runtime, so a build without the renderer still boots.
//
// The lookup is not a plain dlsym(RTLD_DEFAULT): Java loads the library with
// System.loadLibrary, which puts it in the app's namespace but not in the
// global group, and the global group is the only one dlsym(RTLD_DEFAULT)
// searches. Opening the library again returns the very same loaded library -
// the linker finds it by name in the namespace - and a handle can be searched
// directly, whatever group the library lives in.

#include "HostInternal.h"
#include "NativeRenderer.h"
#include <dlfcn.h>
#include <pthread.h>

#define PG_RENDERER_LIBRARY "libmuguet.so"

static void *pgRendererHandle;
static bool pgRendererReported;
static pthread_mutex_t pgRendererLock = PTHREAD_MUTEX_INITIALIZER;

// Looks a renderer symbol up, opening the library the first time it is needed.
void *pg_renderer_symbol(const char *name) {
  pthread_mutex_lock(&pgRendererLock);
  if (!pgRendererHandle) pgRendererHandle = dlopen(PG_RENDERER_LIBRARY, RTLD_NOW);
  void *symbol = pgRendererHandle ? dlsym(pgRendererHandle, name) : NULL;
  if (!symbol) symbol = dlsym(RTLD_DEFAULT, name);
  if (!pgRendererReported) {
    pgRendererReported = true;
    pg_log(symbol ? "Renderer library loaded; the engine draws through it."
                  : "No renderer library in this build: the engine draws nothing.");
  }
  pthread_mutex_unlock(&pgRendererLock);
  return symbol;
}
