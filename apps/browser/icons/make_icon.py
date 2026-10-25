"""Generate the Void app icon (icon.ico + icon.png) with no dependencies.

A gradient superellipse ("squircle") with a white ring: the void.
Run: python apps/browser/icons/make_icon.py
"""

import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).parent
TOP_LEFT = (108, 99, 255)  # oklch(62% 0.2 275)
BOTTOM_RIGHT = (217, 111, 214)  # oklch(70% 0.17 330)
SS = 4  # supersampling per axis


def coverage(size: int, x: int, y: int) -> tuple[float, float]:
    """(squircle coverage, ring coverage) for one pixel."""
    body = ring = 0
    for sy in range(SS):
        for sx in range(SS):
            u = ((x + (sx + 0.5) / SS) / size) * 2 - 1
            v = ((y + (sy + 0.5) / SS) / size) * 2 - 1
            if abs(u * 1.06) ** 5 + abs(v * 1.06) ** 5 <= 1:
                body += 1
                r = math.hypot(u, v)
                if 0.30 <= r <= 0.46:
                    ring += 1
    n = SS * SS
    return body / n, ring / n


def render(size: int) -> bytes:
    rows = []
    for y in range(size):
        row = bytearray([0])  # PNG filter: none
        for x in range(size):
            a, ring = coverage(size, x, y)
            t = (x + y) / (2 * size)
            rgb = [round(c0 + (c1 - c0) * t) for c0, c1 in zip(TOP_LEFT, BOTTOM_RIGHT)]
            if a > 0:
                w = ring / a
                rgb = [round(c + (255 - c) * w) for c in rgb]
            row += bytes(rgb + [round(a * 255)])
        rows.append(bytes(row))
    return png(size, b"".join(rows))


def png(size: int, raw: bytes) -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def ico(images: list[tuple[int, bytes]]) -> bytes:
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, blobs = b"", b""
    for size, data in images:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        blobs += data
    return header + entries + blobs


if __name__ == "__main__":
    images = [(s, render(s)) for s in (16, 24, 32, 48, 64, 128, 256)]
    (OUT / "icon.ico").write_bytes(ico(images))
    (OUT / "icon.png").write_bytes(images[-1][1])
    print("wrote", OUT / "icon.ico", OUT / "icon.png")
