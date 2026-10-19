# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""Generates app/TreeDock.App/Assets/TreeDock.ico (treemap motif) without third-party libraries."""
import pathlib
import struct

OUT = pathlib.Path(__file__).resolve().parent.parent / "app" / "TreeDock.App" / "Assets" / "TreeDock.ico"

# Treemap blocks in a 0..1 unit square: (x0, y0, x1, y1, rgb)
BLOCKS = [
    (0.00, 0.00, 0.58, 0.62, (0x2B, 0x7C, 0xD3)),
    (0.00, 0.62, 0.58, 1.00, (0x56, 0xB4, 0x8C)),
    (0.58, 0.00, 1.00, 0.40, (0xE4, 0xA0, 0x3C)),
    (0.58, 0.40, 1.00, 0.72, (0xD6, 0x60, 0x4D)),
    (0.58, 0.72, 0.80, 1.00, (0x96, 0x6E, 0xC8)),
    (0.80, 0.72, 1.00, 1.00, (0x78, 0x84, 0x96)),
]


def pixel(x, y, n):
    """RGBA for pixel (x, y) of an n×n icon."""
    r = n * 0.18  # corner radius
    gap = max(1.0, n / 32)
    cx = min(max(x + 0.5, r), n - r)
    cy = min(max(y + 0.5, r), n - r)
    if (x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2 > r * r:
        return (0, 0, 0, 0)
    u, v = (x + 0.5) / n, (y + 0.5) / n
    for x0, y0, x1, y1, rgb in BLOCKS:
        if x0 <= u < x1 and y0 <= v < y1:
            # Leave a gap between blocks for the treemap look.
            if (u - x0) * n < gap / 2 and x0 > 0 or (x1 - u) * n < gap / 2 and x1 < 1:
                return (255, 255, 255, 255)
            if (v - y0) * n < gap / 2 and y0 > 0 or (y1 - v) * n < gap / 2 and y1 < 1:
                return (255, 255, 255, 255)
            return (*rgb, 255)
    return (255, 255, 255, 255)


def bmp_entry(n):
    rows = []
    for y in reversed(range(n)):  # bottom-up
        row = b"".join(struct.pack("BBBB", b, g, r, a) for (r, g, b, a) in (pixel(x, y, n) for x in range(n)))
        rows.append(row)
    xor = b"".join(rows)
    and_row = ((n + 31) // 32) * 4
    and_mask = b"\x00" * (and_row * n)
    header = struct.pack("<IiiHHIIiiII", 40, n, n * 2, 1, 32, 0, len(xor) + len(and_mask), 0, 0, 0, 0)
    return header + xor + and_mask


def main():
    sizes = [16, 24, 32, 48, 64, 256]
    images = [bmp_entry(n) for n in sizes]
    out = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    for n, img in zip(sizes, images):
        out += struct.pack("<BBBBHHII", n % 256, n % 256, 0, 0, 1, 32, len(img), offset)
        offset += len(img)
    out += b"".join(images)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(out)
    print(f"wrote {OUT} ({len(out)} bytes)")


if __name__ == "__main__":
    main()
