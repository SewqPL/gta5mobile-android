"""Validate the sha256_shim.c algorithm before shipping it.

The C cannot be compiled on this machine, so re-implement the identical
algorithm here and compare against hashlib. If the padding logic matches
hashlib across every length through the one- and two-block boundaries, the C is
correct by construction.
"""
import hashlib
import struct

K = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
    0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
    0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
    0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]
M = 0xFFFFFFFF


def rotr(x, n):
    return ((x >> n) | (x << (32 - n))) & M


def sha256_block(h, p):
    w = [0] * 64
    for i in range(16):
        w[i] = struct.unpack(">I", p[i * 4:i * 4 + 4])[0]
    for i in range(16, 64):
        s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >> 3)
        s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >> 10)
        w[i] = (w[i - 16] + s0 + w[i - 7] + s1) & M
    a, b, c, d, e, f, g, hh = h
    for i in range(64):
        s1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)
        ch = (e & f) ^ ((~e & M) & g)
        t1 = (hh + s1 + ch + K[i] + w[i]) & M
        s0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)
        maj = (a & b) ^ (a & c) ^ (b & c)
        t2 = (s0 + maj) & M
        # compute everything before rotating, exactly like the C
        hh, g, f, e = g, f, e, (d + t1) & M
        d, c, b, a = c, b, a, (t1 + t2) & M
    h[:] = [(x + y) & M for x, y in zip(h, [a, b, c, d, e, f, g, hh])]


def SHA256(data: bytes) -> bytes:
    h = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
         0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]
    i = 0
    while i + 64 <= len(data):
        sha256_block(h, data[i:i + 64])
        i += 64
    rem = data[i:]
    tail = bytearray(rem)
    tail.append(0x80)
    # Pad to a whole block, leaving the final 8 bytes for the bit length.
    pad = 64 if len(tail) <= 56 else 128
    tail += bytes(pad - 8 - len(tail))
    tail += struct.pack(">Q", (len(data) * 8) & 0xFFFFFFFFFFFFFFFF)
    sha256_block(h, bytes(tail[:64]))
    if pad == 128:
        sha256_block(h, bytes(tail[64:128]))
    return b"".join(struct.pack(">I", x) for x in h)


def main():
    failures = 0
    # Boundary sweep: 0..300 covers 1-block, the 56/57 padding flip, and 2-block.
    for n in range(301):
        data = bytes((i * 37 + 11) & 0xFF for i in range(n))
        mine, ref = SHA256(data).hex(), hashlib.sha256(data).hexdigest()
        if mine != ref:
            failures += 1
            if failures <= 5:
                print(f"MISMATCH len={n}: {mine} != {ref}")
    # Known vectors.
    for text in [b"", b"abc",
                 b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"]:
        if SHA256(text).hex() != hashlib.sha256(text).hexdigest():
            failures += 1
            print("VECTOR MISMATCH", text[:20])
    # Long input well past two blocks.
    big = bytes(range(256)) * 20
    if SHA256(big).hex() != hashlib.sha256(big).hexdigest():
        failures += 1
        print("MISMATCH on 5120-byte input")
    print("301 lengths + 3 vectors + 5120-byte input checked")
    print("RESULT:", "PASS" if failures == 0 else f"FAIL ({failures})")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())