// The engine reads its game files through the HTTP file system's request table:
// the guest posts a slot (file id, offset, destination, length), rings the
// doorbell and waits until the host marks the slot done. The web build serves
// those slots with fetch() in a worker; the Rust build with a thread that reads
// the game folder. This is the same server for the native engine core: the
// game files are on the device, so a read is a pread() from the game folder.
//
// The file id is the index in the manifest's "files" array. The guest is given
// the same manifest (w2c_env_wasm_httpfs_manifest_js), so both sides number
// the files the same way.
//
// ponytail: the engine's prefetch hints (the ring in the table header) are not
// consumed here. They only make the host read ahead of the engine; the demand
// reads below are what the engine waits for. Add the hint ring when the boot
// reads turn out to be the bottleneck.

#include "HostInternal.h"
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define PG_HTTPFS_SLOT_BYTES 64
#define PG_HTTPFS_MAX_SLOTS 4096
#define PG_HTTPFS_WORKERS 4
#define PG_HTTPFS_OPEN_FILES 48
#define PG_HTTPFS_PATH_MAX 1024

typedef struct { char *path; uint64_t size; } PgAssetFile;
// id 0xffffffff: the slot is free; fd -1: the file is not on this device.
typedef struct { uint32_t id; int fd; } PgOpenFile;
typedef struct { PgOpenFile files[PG_HTTPFS_OPEN_FILES]; uint32_t next; } PgFileCache;

static PgAssetFile *pgAssets;
static uint32_t pgAssetCount;
static char pgAssetRoot[PATH_MAX];
static struct w2c_env *pgHttpFsHost;
static uint64_t pgHttpFsTable;
static _Atomic uint64_t pgHttpFsReads, pgHttpFsBytes, pgHttpFsMissing;

static const char *pgSkipSpace(const char *p) {
  while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
  return p;
}

// Reads one JSON string; the escape sequences a file path can carry are
// decoded, everything else is copied as it is.
static const char *pgJsonString(const char *p, char *out, size_t capacity) {
  size_t n = 0;
  while (*p && *p != '"') {
    unsigned char c = (unsigned char)*p++;
    if (c == '\\' && *p) {
      char escape = *p++;
      char extra[2]; int extraCount = 0;
      switch (escape) {
        case 'n': c = '\n'; break;
        case 't': c = '\t'; break;
        case 'r': c = '\r'; break;
        case 'b': c = '\b'; break;
        case 'f': c = '\f'; break;
        case 'u': {
          unsigned code = 0;
          for (int i = 0; i < 4 && *p; i++) {
            char digit = *p++;
            code = code * 16 + (unsigned)(digit >= 'a' ? digit - 'a' + 10 : (digit >= 'A' ? digit - 'A' + 10 : digit - '0'));
          }
          if (code < 0x80) c = (char)code;
          else if (code < 0x800) { extra[0] = (char)(0xC0 | (code >> 6)); extraCount = 1; c = (char)(0x80 | (code & 0x3F)); }
          else { extra[0] = (char)(0xE0 | (code >> 12)); extra[1] = (char)(0x80 | ((code >> 6) & 0x3F)); extraCount = 2; c = (char)(0x80 | (code & 0x3F)); }
          break;
        }
        default: c = (unsigned char)escape; break;
      }
      for (int i = 0; i < extraCount; i++) if (n + 1 < capacity) out[n++] = extra[i];
    }
    if (n + 1 < capacity) out[n++] = (char)c;
  }
  if (*p == '"') p++;
  if (capacity) out[n < capacity ? n : capacity - 1] = 0;
  return p;
}

// Reads the game's file list: the manifest the guest is also given, so the
// file ids mean the same thing on both sides.
void pg_httpfs_configure(const char *manifestPath) {
  if (!manifestPath) return;
  FILE *file = fopen(manifestPath, "rb");
  if (!file) {
    pg_log("The game file list could not be opened; the engine will not find its files.");
    return;
  }
  fseek(file, 0, SEEK_END);
  long length = ftell(file);
  rewind(file);
  if (length <= 0) { fclose(file); return; }
  char *text = malloc((size_t)length + 1);
  if (!text || fread(text, 1, (size_t)length, file) != (size_t)length) {
    free(text); fclose(file);
    pg_log("The game file list could not be read.");
    return;
  }
  fclose(file);
  text[length] = 0;
  // The manifest sits in the game's data folder and its paths are relative to it.
  snprintf(pgAssetRoot, sizeof pgAssetRoot, "%s", manifestPath);
  char *slash = strrchr(pgAssetRoot, '/');
  if (slash) *slash = 0;
  else snprintf(pgAssetRoot, sizeof pgAssetRoot, ".");
  const char *p = strstr(text, "\"files\"");
  if (!p) { free(text); pg_log("The game file list has no file array."); return; }
  p = pgSkipSpace(p + 7);
  if (*p != ':') { free(text); return; }
  p = pgSkipSpace(p + 1);
  if (*p != '[') { free(text); return; }
  p++;
  PgAssetFile *files = NULL;
  uint32_t count = 0, capacity = 0;
  char path[PG_HTTPFS_PATH_MAX];
  for (;;) {
    p = pgSkipSpace(p);
    if (*p != '[') break;
    p = pgSkipSpace(p + 1);
    if (*p != '"') break;
    p = pgJsonString(p + 1, path, sizeof path);
    p = pgSkipSpace(p);
    if (*p != ',') break;
    p = pgSkipSpace(p + 1);
    char *end = NULL;
    unsigned long long size = strtoull(p, &end, 10);
    if (end == p) break;
    p = pgSkipSpace(end);
    if (*p == ',') {                       // the modification time is not used
      p = pgSkipSpace(p + 1);
      unsigned long long stamp = strtoull(p, &end, 10);
      if (end != p) p = pgSkipSpace(end);
      (void)stamp;
    }
    if (*p != ']') break;
    p = pgSkipSpace(p + 1);
    if (count == capacity) {
      capacity = capacity ? capacity * 2 : 2048;
      PgAssetFile *grown = realloc(files, capacity * sizeof *grown);
      if (!grown) break;
      files = grown;
    }
    files[count].path = strdup(path);
    files[count].size = size;
    if (!files[count].path) break;
    count++;
    if (*p != ',') break;
    p++;
  }
  free(text);
  pgAssets = files;
  pgAssetCount = count;
  char line[256];
  snprintf(line, sizeof line, "Game file list: %u files in %s.", count, pgAssetRoot);
  pg_log(line);
}

// Opens the game file, keeping the last few open so the engine's many small
// reads do not reopen the same archive every time.
static int pgOpenAsset(PgFileCache *cache, uint32_t id) {
  for (uint32_t i = 0; i < PG_HTTPFS_OPEN_FILES; i++)
    if (cache->files[i].id == id) return cache->files[i].fd;
  char path[PATH_MAX];
  snprintf(path, sizeof path, "%s/%s", pgAssetRoot, pgAssets[id].path);
  int fd = open(path, O_RDONLY | O_CLOEXEC);
  if (fd < 0) {
    if (atomic_fetch_add(&pgHttpFsMissing, 1) < 8) {
      char line[PATH_MAX + 96];
      snprintf(line, sizeof line, "Game file missing on this device: %s", pgAssets[id].path);
      pg_log(line);
    }
  }
  uint32_t slot = cache->next++ % PG_HTTPFS_OPEN_FILES;
  if (cache->files[slot].fd >= 0) close(cache->files[slot].fd);
  cache->files[slot].id = id;
  cache->files[slot].fd = fd;
  return fd;
}

// Copies the requested range of a game file into the engine's memory. The
// result is the number of bytes read, or -1 when the file could not be read.
static int64_t pgReadAsset(PgFileCache *cache, uint32_t id, uint64_t offset,
                           uint64_t destination, uint32_t length) {
  if (id >= pgAssetCount) return -1;
  PgAssetFile *entry = &pgAssets[id];
  if (!length || offset >= entry->size) return 0;
  if ((uint64_t)length > entry->size - offset) length = (uint32_t)(entry->size - offset);
  wasm_rt_shared_memory_t *memory = w2c_env_memory(pgHttpFsHost);
  if (destination > memory->size || length > memory->size - destination) return -1;
  uint8_t *target = (uint8_t *)(void *)memory->data + destination;
  int fd = pgOpenAsset(cache, id);
  if (fd < 0) return -1;
  uint64_t done = 0;
  while (done < length) {
    ssize_t n = pread(fd, target + done, length - done, (off_t)(offset + done));
    if (n < 0) { if (errno == EINTR) continue; return -1; }
    if (!n) break;
    done += (uint64_t)n;
  }
  atomic_fetch_add(&pgHttpFsReads, 1);
  atomic_fetch_add(&pgHttpFsBytes, done);
  return (int64_t)done;
}

static void *pgHttpFsWorker(void *unused) {
  (void)unused;
  wasm_rt_shared_memory_t *memory = w2c_env_memory(pgHttpFsHost);
  PgFileCache cache;
  memset(&cache, 0, sizeof cache);
  for (uint32_t i = 0; i < PG_HTTPFS_OPEN_FILES; i++) { cache.files[i].id = 0xffffffffu; cache.files[i].fd = -1; }
  for (;;) {
    if (pgHttpFsTable + PG_HTTPFS_SLOT_BYTES > memory->size) {
      pg_log("The engine's file request table is outside its memory.");
      return NULL;
    }
    uint8_t *base = (uint8_t *)(void *)memory->data;
    _Atomic int32_t *header = (_Atomic int32_t *)(void *)(base + pgHttpFsTable);
    int32_t bell = atomic_load(header);
    int32_t count = atomic_load(header + 1);
    if (count < 0 || count > PG_HTTPFS_MAX_SLOTS) {
      pg_log("The engine asked for an impossible number of file slots.");
      return NULL;
    }
    for (int32_t i = 0; i < count; i++) {
      uint64_t address = pgHttpFsTable + PG_HTTPFS_SLOT_BYTES + PG_HTTPFS_SLOT_BYTES * (uint64_t)i;
      if (address + PG_HTTPFS_SLOT_BYTES > memory->size) break;
      _Atomic int32_t *slot = (_Atomic int32_t *)(void *)(base + address);
      int32_t expected = 1;
      if (!atomic_compare_exchange_strong(slot, &expected, 3)) continue;
      uint32_t id = (uint32_t)atomic_load(slot + 1);
      uint64_t offset = (uint32_t)atomic_load(slot + 2) | ((uint64_t)(uint32_t)atomic_load(slot + 3) << 32);
      uint64_t destination = (uint32_t)atomic_load(slot + 4) | ((uint64_t)(uint32_t)atomic_load(slot + 5) << 32);
      uint32_t length = (uint32_t)atomic_load(slot + 6);
      int64_t result = pgReadAsset(&cache, id, offset, destination, length);
      atomic_store(slot + 7, (int32_t)result);
      atomic_store(slot, 2);
      pg_atomic_notify(memory, address, UINT32_MAX);
    }
    uint64_t reads = atomic_load(&pgHttpFsReads);
    if (reads && reads % 20000 == 0) {
      char line[160];
      snprintf(line, sizeof line, "Native file server: %llu reads, %llu MB.",
               (unsigned long long)reads, (unsigned long long)(atomic_load(&pgHttpFsBytes) >> 20));
      pg_log(line);
    }
    pg_atomic_wait32(memory, pgHttpFsTable, (uint32_t)bell, 20000000);
  }
  return NULL;
}

// The engine hands over its request table when the file worker starts.
void pg_httpfs_start(struct w2c_env *host, uint64_t table) {
  if (pgHttpFsTable) return;
  pgHttpFsHost = host;
  pgHttpFsTable = table;
  if (!pgAssetCount) {
    pg_log("The game file list is empty; the engine will not find its files.");
    return;
  }
  char line[160];
  snprintf(line, sizeof line, "Native file server attached: %u game files on this device.", pgAssetCount);
  pg_log(line);
  for (int i = 0; i < PG_HTTPFS_WORKERS; i++) {
    pthread_attr_t attributes;
    pthread_attr_init(&attributes);
    pthread_attr_setstacksize(&attributes, 1024 * 1024);
    pthread_t thread;
    int result = pthread_create(&thread, &attributes, pgHttpFsWorker, NULL);
    pthread_attr_destroy(&attributes);
    if (result) { pg_log("Could not create a native file server thread."); continue; }
    pthread_detach(thread);
  }
}
