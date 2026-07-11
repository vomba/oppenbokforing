#!/usr/bin/env bash
# Generate branded dev placeholder icons for Tauri compile/bundle.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ICONS="$ROOT/src-tauri/icons"

python3 - <<'PY'
import struct
import zlib
from pathlib import Path

BG = (15, 52, 44, 255)
ACCENT = (212, 175, 55, 255)
TEXT = (248, 250, 249, 255)


def chunk(tag: bytes, data: bytes) -> bytes:
    crc = zlib.crc32(tag + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)


def png(width: int, height: int, pixels: bytes) -> bytes:
    rows = []
    stride = width * 4
    for y in range(height):
        rows.append(b"\x00" + pixels[y * stride : (y + 1) * stride])
    compressed = zlib.compress(b"".join(rows), 9)
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", compressed) + chunk(b"IEND", b"")


def fill_rect(pixels, width, x0, y0, x1, y1, color):
    for y in range(max(0, y0), min(height := len(pixels) // (width * 4), y1)):
        for x in range(max(0, x0), min(width, x1)):
            i = (y * width + x) * 4
            pixels[i : i + 4] = bytes(color)


def draw_icon(size: int) -> bytes:
    pixels = bytearray(BG * size * size)
    margin = max(2, size // 8)
    fill_rect(pixels, size, margin, margin, size - margin, size - margin, ACCENT)
    inner = margin + max(1, size // 10)
    fill_rect(pixels, size, inner, inner, size - inner, size - inner, BG)

    # Simple "S" mark using blocks
    block = max(1, size // 10)
    cx = size // 2 - block
    cy = size // 2 - block * 2
    for dx in range(block * 3):
        for dy in range(block * 5):
            x = cx + dx
            y = cy + dy
            draw = (
                (dy < block and dx >= block)
                or (block <= dy < block * 2 and dx < block * 2)
                or (block * 2 <= dy < block * 3 and dx >= 0)
                or (dy >= block * 3 and dx < block * 2)
            )
            if draw:
                i = (y * size + x) * 4
                if 0 <= i < len(pixels) - 3:
                    pixels[i : i + 4] = bytes(TEXT)

    return png(size, size, bytes(pixels))


icons = Path("src-tauri/icons")
icons.mkdir(parents=True, exist_ok=True)
for size, name in [(32, "32x32.png"), (128, "128x128.png"), (256, "128x128@2x.png"), (512, "icon.png")]:
    (icons / name).write_bytes(draw_icon(size))
print(f"Wrote branded dev icons to {icons}")
PY

echo "Dev icons ready in $ICONS"
