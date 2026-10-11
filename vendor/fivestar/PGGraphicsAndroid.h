#pragma once
// Android graphics seam for the vendored engine core.
//
// This replaces the Metal renderer's PGAttachMetalLayer(void *CAMetalLayer).
// The upstream contract hands the renderer a drawable-producing object; on
// Android that role belongs to a native window surfaced through Vulkan.
//
// Two implementations are wired by the NDK build:
//   - libgta5vk.so:  a real Vulkan backend (the port target)
//   - a headless stub: swaps a 1x1 image, so the engine, files, audio and
//     threading can be validated on-device before the renderer exists.
//
// Nothing in the vendored C core includes Metal; it only calls the four
// functions below. Swapping which one is linked changes nothing above this line.
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Attaches the surface the renderer presents to. On Android `window` is an
// ANativeWindow* obtained from SurfaceHolder.getSurface(). Passing NULL detaches.
void PGAttachVulkanWindow(void *window);

// Starts the renderer thread against the engine's command ring.
// memory/memorySize are the engine heap; ring/ringSize are the graphics
// command ring the engine writes into.
void PGStartGraphics(void *memory, uint64_t memorySize,
                     uint64_t ring, uint64_t ringSize, uint64_t debug);

double PGRenderedFramesPerSecond(void);

// Writes a single PNG of the last presented frame, for bug reports.
void PGCaptureNativeFrame(const char *name);

#ifdef __cplusplus
}
#endif