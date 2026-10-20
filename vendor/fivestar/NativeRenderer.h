#pragma once
// The renderer (the Rust library, libmuguet.so) is resolved at runtime: the
// engine core never links against it. See NativeRenderer.c for how it is
// found.
void *pg_renderer_symbol(const char *name);
