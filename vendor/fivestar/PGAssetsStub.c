// The upstream asset server (app/Source/NativeAssets.m) is Objective-C over
// Foundation: it serves the engine's file reads through the game's manifest,
// overlays the mod folder and keeps a packed asset cache. Android has no
// Foundation, and this port's file IO (NativeFiles.c) reads the game folder
// directly, so the entry points the engine core calls are no-ops here.
//
// What this drops: the manifest overlay, so the engine core cannot serve
// modded files yet. Everything else (boot, streaming, audio, input, the
// graphics ring) is unaffected.
//
// ponytail: if mod support is wanted on Android, this is where the manifest
// lookup belongs; it needs a C implementation of the upstream server.

#include <stddef.h>

// The engine core calls this at boot with the manifest path and the game root.
void pg_assets_configure(const char *manifest, const char *root) {
  (void)manifest;
  (void)root;
}

// The engine core calls this after boot to warm the file cache.
void pg_assets_prewarm(void) {}

// The manifest the engine should read instead of the game's own: none.
const char *pg_assets_manifest_override(void) { return NULL; }
