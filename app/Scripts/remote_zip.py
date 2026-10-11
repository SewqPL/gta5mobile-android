"""Read a remote ZIP's central directory over HTTP range requests and pull out
selected entries, without downloading the whole archive.

The archive here is ~18.9 GB; we only need b/<build>/game.wasm (~63 MB).
"""
from __future__ import annotations

import argparse
import io
import struct
import sys
import urllib.request

URL = "https://gcode.bytarch.com/extracted.zip"
UA = "Mozilla/5.0 (range-reader)"


def http_range(url: str, start: int, length: int) -> bytes:
    """Fetch `length` bytes starting at `start`."""
    end = start + length - 1
    request = urllib.request.Request(
        url, headers={"Range": f"bytes={start}-{end}", "User-Agent": UA}
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        if response.status != 206:
            raise RuntimeError(f"server ignored Range (status {response.status})")
        return response.read()


def remote_size(url: str) -> int:
    request = urllib.request.Request(url, method="HEAD", headers={"User-Agent": UA})
    with urllib.request.urlopen(request, timeout=60) as response:
        return int(response.headers["Content-Length"])


def find_eocd(url: str, size: int) -> tuple[int, int]:
    """Return (central_directory_offset, central_directory_size), ZIP64 aware."""
    # Pull a wide tail so the locator/EOCD records are comfortably inside it, and
    # the classic EOCD (which follows the locator) never falls off the end.
    tail_len = min(size, 1024 * 1024)
    tail = http_range(url, size - tail_len, tail_len)

    # ZIP64 end-of-central-directory locator sits just before the classic EOCD.
    locator = tail.rfind(b"PK\x06\x07")
    if locator >= 0 and locator + 20 <= len(tail):
        _, zip64_eocd_offset = struct.unpack("<IQ", tail[locator + 4: locator + 20])
        record = http_range(url, zip64_eocd_offset, 56)
        if record[:4] != b"PK\x06\x06":
            raise RuntimeError("bad ZIP64 EOCD signature")
        # entries, entries_total, cd_size, cd_offset are 8 bytes each at 24/32/40/48
        cd_size, cd_offset = struct.unpack("<QQ", record[40:56])
        return cd_offset, cd_size

    eocd = tail.rfind(b"PK\x05\x06")
    if eocd < 0:
        raise RuntimeError("no end-of-central-directory record found")
    cd_size, cd_offset = struct.unpack("<II", tail[eocd + 12: eocd + 20])
    if cd_offset == 0xFFFFFFFF or cd_size == 0xFFFFFFFF:
        raise RuntimeError("archive looks ZIP64 but no locator was found")
    return cd_offset, cd_size


def iter_central_directory(url: str, cd_offset: int, cd_size: int):
    """Yield (name, compressed_size, uncompressed_size, local_header_offset)."""
    signature = b"PK\x01\x02"
    buffer = b""
    position = cd_offset
    end = cd_offset + cd_size
    while position < end:
        want = min(1024 * 1024, end - position)
        chunk = http_range(url, position, want)
        position += len(chunk)
        buffer += chunk
        while True:
            index = buffer.find(signature)
            if index < 0:
                break
            header = buffer[index: index + 46]
            if len(header) < 46:
                break
            (
                _sig, _ver, _need, _flags, method, _time, _date, _crc,
                comp_size, uncomp_size, name_len, extra_len, _comment_len,
                _disk, _int_attr, _ext_attr, local_offset,
            ) = struct.unpack("<IHHHHHHIIIHHHHHII", header)
            name = buffer[index + 46: index + 46 + name_len].decode("utf-8", "replace")
            extra = buffer[index + 46 + name_len: index + 46 + name_len + extra_len]
            # ZIP64 extended information overrides the 32-bit fields.
            if 0xFFFFFFFF in (comp_size, uncomp_size, local_offset):
                cursor = 0
                while cursor + 4 <= len(extra):
                    tag, size = struct.unpack("<HH", extra[cursor: cursor + 4])
                    body = extra[cursor + 4: cursor + 4 + size]
                    if tag == 0x0001:
                        offset = 0
                        if uncomp_size == 0xFFFFFFFF:
                            uncomp_size = struct.unpack("<Q", body[offset: offset + 8])[0]
                            offset += 8
                        if comp_size == 0xFFFFFFFF:
                            comp_size = struct.unpack("<Q", body[offset: offset + 8])[0]
                            offset += 8
                        if local_offset == 0xFFFFFFFF:
                            local_offset = struct.unpack("<Q", body[offset: offset + 8])[0]
                        break
                    cursor += 4 + size
            buffer = buffer[index + 46 + name_len + extra_len:]
            yield name, comp_size, uncomp_size, local_offset, method


def extract_entry(url: str, comp_size: int, local_offset: int, method: int, out_path: str) -> int:
    """Fetch one entry's bytes by range and write it out."""
    local = http_range(url, local_offset, 30)
    if local[:4] != b"PK\x03\x04":
        raise RuntimeError("bad local file header")
    name_len, extra_len = struct.unpack("<HH", local[26:30])
    data_start = local_offset + 30 + name_len + extra_len
    payload = http_range(url, data_start, comp_size)
    if method == 0:
        data = payload
    elif method == 8:
        import zlib
        data = zlib.decompress(payload, -15)
    else:
        raise RuntimeError(f"unsupported compression method {method}")
    with open(out_path, "wb") as handle:
        handle.write(data)
    return len(data)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default=URL)
    parser.add_argument("--list", default="game.wasm")
    parser.add_argument("--extract", help="entry name to write to this path")
    parser.add_argument("--out", help="output path for --extract")
    options = parser.parse_args()

    size = remote_size(options.url)
    print(f"archive: {size:,} bytes ({size/1e9:.1f} GB)")

    cd_offset, cd_size = find_eocd(options.url, size)
    print(f"central directory: offset {cd_offset:,}, size {cd_size:,} ({cd_size/1e6:.1f} MB)")

    print("scanning for matches...")
    matches = []
    total_entries = 0
    for name, comp_size, uncomp_size, local_offset, method in iter_central_directory(
        options.url, cd_offset, cd_size
    ):
        total_entries += 1
        if options.list in name:
            matches.append((name, comp_size, uncomp_size, local_offset, method))
        if total_entries % 100000 == 0:
            print(f"  ...{total_entries:,} entries scanned")
    print(f"{total_entries:,} entries total, {len(matches)} match '{options.list}'\n")

    for name, comp_size, uncomp_size, local_offset, method in matches[:50]:
        print(f"  {name}")
        print(f"      compressed {comp_size:,}  uncompressed {uncomp_size:,}  method {method}")

    if options.extract:
        target = next((m for m in matches if options.extract in m[0]), None)
        if not target:
            print(f"\nno entry matching '{options.extract}'", file=sys.stderr)
            return 1
        name, comp_size, uncomp_size, local_offset, method = target
        out = options.out or name.replace("/", "_")
        print(f"\nextracting {name} -> {out}")
        written = extract_entry(options.url, comp_size, local_offset, method, out)
        print(f"wrote {written:,} bytes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())