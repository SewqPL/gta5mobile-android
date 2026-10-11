// Portable SHA-256 for the host wasm2c build.
//
// wabt's prebuilt libwabt.a references OpenSSL's one-shot SHA256() but does not
// carry the object that defines it, so linking wasm2c-native fails with
// "undefined reference to SHA256". Upstream solves this with a CommonCrypto
// shim, which only works on macOS; this one is plain C and works anywhere.
//
// This is the same algorithm the engine build already relies on: the generated
// game sources are checksummed, so a wrong digest would corrupt them silently.

#include <stddef.h>
#include <stdint.h>
#include <string.h>

static const uint32_t K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
};

#define ROTR(x, n) (((x) >> (n)) | ((x) << (32 - (n))))

static void sha256_block(uint32_t h[8], const uint8_t *p) {
    uint32_t w[64], a, b, c, d, e, f, g, hh;
    int i;
    for (i = 0; i < 16; i++) {
        w[i] = ((uint32_t)p[i * 4] << 24) | ((uint32_t)p[i * 4 + 1] << 16) |
               ((uint32_t)p[i * 4 + 2] << 8) | (uint32_t)p[i * 4 + 3];
    }
    for (i = 16; i < 64; i++) {
        uint32_t s0 = ROTR(w[i - 15], 7) ^ ROTR(w[i - 15], 18) ^ (w[i - 15] >> 3);
        uint32_t s1 = ROTR(w[i - 2], 17) ^ ROTR(w[i - 2], 19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16] + s0 + w[i - 7] + s1;
    }
    a = h[0]; b = h[1]; c = h[2]; d = h[3];
    e = h[4]; f = h[5]; g = h[6]; hh = h[7];
    for (i = 0; i < 64; i++) {
        uint32_t s1 = ROTR(e, 6) ^ ROTR(e, 11) ^ ROTR(e, 25);
        uint32_t ch = (e & f) ^ ((~e) & g);
        uint32_t t1 = hh + s1 + ch + K[i] + w[i];
        uint32_t s0 = ROTR(a, 2) ^ ROTR(a, 13) ^ ROTR(a, 22);
        uint32_t maj = (a & b) ^ (a & c) ^ (b & c);
        uint32_t t2 = s0 + maj;
        hh = g; g = f; f = e; e = d + t1;
        d = c; c = b; b = a; a = t1 + t2;
    }
    h[0] += a; h[1] += b; h[2] += c; h[3] += d;
    h[4] += e; h[5] += f; h[6] += g; h[7] += hh;
}

unsigned char *SHA256(const unsigned char *data, size_t length, unsigned char *digest) {
    uint32_t h[8] = {
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    };
    uint8_t tail[128];
    uint64_t bits;
    size_t i = 0, rem, pad;
    int j;

    for (; i + 64 <= length; i += 64) sha256_block(h, data + i);

    rem = length - i;
    memcpy(tail, data + i, rem);
    tail[rem++] = 0x80;
    /* Pad to a whole block, leaving the final 8 bytes for the bit length. */
    pad = (rem <= 56) ? 64 : 128;
    memset(tail + rem, 0, pad - 8 - rem);
    bits = (uint64_t)length * 8;
    for (j = 0; j < 8; j++) tail[pad - 1 - j] = (uint8_t)(bits >> (8 * j));
    sha256_block(h, tail);
    if (pad == 128) sha256_block(h, tail + 64);

    for (j = 0; j < 8; j++) {
        digest[j * 4]     = (uint8_t)(h[j] >> 24);
        digest[j * 4 + 1] = (uint8_t)(h[j] >> 16);
        digest[j * 4 + 2] = (uint8_t)(h[j] >> 8);
        digest[j * 4 + 3] = (uint8_t)h[j];
    }
    return digest;
}

#ifdef SHA256_SHIM_TEST
#include <stdio.h>
int main(void) {
    /* NIST vectors plus the boundary lengths the padding path can get wrong. */
    static const char *vectors[] = {
        "",
        "abc",
        "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
    };
    size_t lengths[] = { 0, 3, 56 };
    int i;
    unsigned char digest[32];
    for (i = 0; i < 3; i++) {
        SHA256((const unsigned char *)vectors[i], lengths[i], digest);
        for (int j = 0; j < 32; j++) printf("%02x", digest[j]);
        printf("  \"%s\"\n", vectors[i]);
    }
    /* Sweep every length through one and two block pads. */
    unsigned char buf[200];
    for (i = 0; i < 200; i++) buf[i] = (unsigned char)i;
    for (i = 0; i < 200; i++) {
        SHA256(buf, (size_t)i, digest);
        printf("%d ", i);
        for (int j = 0; j < 4; j++) printf("%02x", digest[j]);
        printf("\n");
    }
    return 0;
}
#endif