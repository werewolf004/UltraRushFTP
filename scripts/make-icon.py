# Draws the application icon: a white lightning bolt on a rounded blue square.
#
# Pure Python (no imaging library): every pixel is supersampled 4 x 4 for smooth edges,
# written as PNG with zlib, and the sizes Windows asks for are gathered in one .ico
# (PNG-compressed entries, Windows Vista and later).
#
#   python -I scripts/make-icon.py
# Writes crates/urftp-gui/assets/UltraRushFTP.ico and crates/urftp-gui/assets/icon-256.png.
import os
import struct
import zlib

SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
SAMPLES = 4

TOP = (0x3B, 0x7B, 0xE8)     # gradient of the square, top to bottom
BOTTOM = (0x1F, 0x56, 0xC2)  # = accent of the light theme
BOLT = (0xFF, 0xFF, 0xFF)

# Shapes in a unit square (y downwards).
MARGIN = 0.03
RADIUS = 0.22
BOLT_POINTS = [(0.57, 0.08), (0.74, 0.08), (0.59, 0.40), (0.77, 0.40),
               (0.37, 0.93), (0.47, 0.55), (0.26, 0.55)]


def in_square(x, y):
    lo, hi = MARGIN, 1.0 - MARGIN
    if not (lo <= x <= hi and lo <= y <= hi):
        return False
    cx = min(max(x, lo + RADIUS), hi - RADIUS)
    cy = min(max(y, lo + RADIUS), hi - RADIUS)
    return (x - cx) ** 2 + (y - cy) ** 2 <= RADIUS ** 2


def in_polygon(x, y, pts):
    inside = False
    j = len(pts) - 1
    for i in range(len(pts)):
        xi, yi = pts[i]
        xj, yj = pts[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            inside = not inside
        j = i
    return inside


def render(size):
    rows = []
    # Small sizes: a thicker bolt reads better than a faithful one.
    scale = 1.08 if size <= 24 else 1.0
    pts = [(0.5 + (px - 0.5) * scale, 0.5 + (py - 0.5) * scale) for px, py in BOLT_POINTS]
    for py in range(size):
        row = bytearray([0])  # PNG filter: none
        for px in range(size):
            square = bolt = 0
            for sy in range(SAMPLES):
                for sx in range(SAMPLES):
                    x = (px + (sx + 0.5) / SAMPLES) / size
                    y = (py + (sy + 0.5) / SAMPLES) / size
                    if in_square(x, y):
                        square += 1
                        if in_polygon(x, y, pts):
                            bolt += 1
            n = SAMPLES * SAMPLES
            if square == 0:
                row += bytes(4)
                continue
            t = py / max(size - 1, 1)
            base = [round(a + (b - a) * t) for a, b in zip(TOP, BOTTOM)]
            k = bolt / square
            rgb = [round(c + (w - c) * k) for c, w in zip(base, BOLT)]
            row += bytes(rgb + [round(255 * square / n)])
        rows.append(bytes(row))
    return b"".join(rows)


def png(size, raw):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)  # 8-bit RGBA
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def ico(images):
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    body = b""
    for size, data in images:
        dim = 0 if size >= 256 else size
        out += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        body += data
    return out + body


def main():
    root = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    assets = os.path.join(root, "crates", "urftp-gui", "assets")
    os.makedirs(assets, exist_ok=True)
    images = [(s, png(s, render(s))) for s in SIZES]
    with open(os.path.join(assets, "UltraRushFTP.ico"), "wb") as f:
        f.write(ico(images))
    with open(os.path.join(assets, "icon-256.png"), "wb") as f:
        f.write(dict(images)[256])
    print("ok")


if __name__ == "__main__":
    main()
