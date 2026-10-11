# Builds the vendored GTA V engine core (wasm2c output + host) for Android arm64.
#
# The upstream iOS project builds this same C core with a patched wasm2c and a set
# of defines in project.yml. We reproduce those defines here so the vendored
# sources compile identically on Android; the only platform deltas live in
# vendor/fivestar/PGPort.h (the three Apple APIs) and the graphics seam.
#
# Game code is produced by app/Scripts/prepare_android.py from your own game.wasm
# and is NOT checked in. That generated tree (game.h + game_*.c + the wabt wasm2c
# runtime) is REQUIRED: every host translation unit includes HostInternal.h, which
# includes the generated game.h. There is no meaningful host-only build - without
# game files this target does not compile, and that is by design, not an oversight.

cmake_minimum_required(VERSION 3.22)
project(gta5engine C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(REPO_ROOT ${CMAKE_CURRENT_SOURCE_DIR}/../..)
set(GENERATED_DIR ${REPO_ROOT}/app/Generated)
set(GAME_LIST ${GENERATED_DIR}/game_sources.cmake)
if(NOT EXISTS ${GAME_LIST})
  message(FATAL_ERROR
    "Generated game code missing. Run first:\n"
    "  python3 app/Scripts/prepare_android.py --mirror <folder-with-data-and-b>\n"
    "The engine core is pinned to wasm2c build 8b0b5899ed (sha256 "
    "11ca8d2c...83e0); prepare_android.py verifies it.")
endif()
include(${GAME_LIST})

set(HOST_SOURCES
  ${REPO_ROOT}/vendor/fivestar/EngineHost.c
  ${REPO_ROOT}/vendor/fivestar/NativeFiles.c
  ${REPO_ROOT}/vendor/fivestar/NativeThreads.c
  ${REPO_ROOT}/vendor/fivestar/NativeProxy.c
  ${REPO_ROOT}/vendor/fivestar/WasmFutex.c
  ${REPO_ROOT}/vendor/fivestar/BCTextureDecoder.c
  ${REPO_ROOT}/vendor/fivestar/PGGraphicsHeadless.c
)

# The JNI bridge is C, not C++: it links against the same engine core and exposes
# it to the Java shell (dev.muguet.EngineActivity). Compiled into the same .so.
set(BRIDGE_SOURCE ${REPO_ROOT}/vendor/fivestar/EngineBridge.c)

add_library(gta5engine SHARED ${HOST_SOURCES} ${BRIDGE_SOURCE} ${GAME_CODE_SOURCES})

target_include_directories(gta5engine PRIVATE
  ${REPO_ROOT}/vendor/fivestar
  ${GENERATED_DIR}
  # The wasm2c runtime (wasm-rt-*.c) comes from wabt; simde supplies the x86 SIMD
  # intrinsics the generated code uses on arm64.
  ${REPO_ROOT}/app/Tools/simde
  ${REPO_ROOT}/app/Tools/wabt-1.0.42/include
  ${REPO_ROOT}/app/Tools/wabt-1.0.42/share/wabt/wasm2c
)

target_compile_options(gta5engine PRIVATE
  -Wno-deprecated-declarations
  -frounding-math
  -fno-optimize-sibling-calls
)
target_compile_definitions(gta5engine PRIVATE
  WASM_RT_USE_MMAP=0
  WASM_RT_MEMCHECK_BOUNDS_CHECK=1
  WASM_RT_SKIP_SIGNAL_RECOVERY=1
  WASM_RT_MAX_CALL_STACK_DEPTH=10000
)
# Force-include the runtime shims exactly like upstream (NativeRuntime.h pulls in
# WasmFutex.h and SharedSIMD.h).
target_compile_options(gta5engine PRIVATE -include ${REPO_ROOT}/vendor/fivestar/NativeRuntime.h)
target_link_libraries(gta5engine PRIVATE log dl m android)