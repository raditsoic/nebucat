#!/usr/bin/env python3
"""Render the nebucat pixel logo into assets/logo/nebucat.svg and assets/icon/."""

import math
import os
import struct
import zlib

SIZES = (16, 24, 32, 48, 64, 256)

PALETTE = {
    "K": "#15141B",
    "M": "#9A5BC0",
    "P": "#B79CFF",
    "D": "#8B6BE0",
    "L": "#E4D7FF",
    "R": "#FF7AC6",
    "Q": "#C9509A",
    "W": "#ECEAF2",
}

# One character per pixel; "." is transparent. Eyes use the background color "K".
GRID = (
    "....KKKKKKKKKKKKKKKKKKKKKKKK....",
    "...KKKKKKKKKKKKKKKKKKKKKKKKKK...",
    "..KKKKKKKKKKKKKKKKKKKKKKKKKKKK..",
    ".KKKKKKKKKKKKKKKKKKKKKKKKKKKKKK.",
    "KKKKKKKKKKKKKKKKKKKKKKKKKKWKKKKK",
    "KKKKKKKKKPKKKKKKKKKKKKPKKKKKKKKK",
    "KKKKKWKKKPPKKKKKKKKKKPPKKKKKKKKK",
    "KKKKWWWKKPRPKKKKKKKKPRPKKKKKKKKK",
    "KKKKKWKKKPRRPKKKKKKPRRPKKKKKKKKK",
    "KKKKKKKKKPPPPPPPPPPPPPPKKKKKKKKK",
    "KKKKKKKKPPPPPPPPPPPPPPPDKKKKKKKK",
    "KKKKKKKKPPLLPPPPPPPPPPPDKKKKKKKK",
    "KKKKKKKPPLPPPPPPPPPPPPPPDKKKKKKK",
    "KKKKKKKPPPPPPPPPPPPPPPPPDMMMKKKK",
    "KKKKKKKPPPPPPPPPPPPPPPPDDKKKMMKK",
    "KKKKKKKPPPPKWPPPPPPKWPPDDKKKKKRK",
    "KKKKKKKPPPPKKPPPPPPKKPPDDKKKKRKK",
    "KKKKKKKPPPPPPPPPPPPPPPPDDKKRRKKK",
    "KKKKKMMPPPPPPPPRRPPPPPPDRRRQQKKK",
    "KKKMMKKPPPPPPPPPPPPPPRRRQQQKKKKK",
    "KKMKKKKPPPPPPPPPPRRRRQQQDKKKKKKK",
    "KRKKKKKDPPPPRRRRRQQQQPPDDKKKKKKK",
    "KKRRRRRRRRRRQQQQQPPPPPDDKKKKKKKK",
    "KKQQQQQQQQQQPPPPPPPPPPDDKKKKKKKK",
    "KKKKKKKKKDPPPPDDDDDDDPDKKKKKKKKK",
    "KKKKKKKKKKKDDDDDDDDDDKKKKKKKKKKK",
    "KKKKKKKKKKKKKKKKKKKKKKKKKKKWKKKK",
    "KKKKWKKKKKKKKKKKKKKKKKKKKKKKKKKK",
    ".KKKKKKKKKKKKKKKKKKKKKKKKKKKKKK.",
    "..KKKKKKKKKKKKKKKKKKKKKKKKKKKK..",
    "...KKKKKKKKKKKKKKKKKKKKKKKKKK...",
    "....KKKKKKKKKKKKKKKKKKKKKKKK....",
)
GRID_SIZE = len(GRID)


def hex_rgb(value):
    return tuple(int(value[i:i + 2], 16) / 255 for i in (1, 3, 5))


def render_master():
    # Channels are premultiplied by alpha so box filtering blends edges correctly.
    n = GRID_SIZE * GRID_SIZE
    r, g, b, a = [0.0] * n, [0.0] * n, [0.0] * n, [0.0] * n
    for y, row in enumerate(GRID):
        for x, key in enumerate(row):
            if key == ".":
                continue
            i = y * GRID_SIZE + x
            r[i], g[i], b[i] = hex_rgb(PALETTE[key])
            a[i] = 1.0
    return [r, g, b, a]


def render_svg():
    paths = {key: [] for key in PALETTE}
    for y, row in enumerate(GRID):
        x = 0
        while x < GRID_SIZE:
            key = row[x]
            start = x
            while x < GRID_SIZE and row[x] == key:
                x += 1
            if key != ".":
                width = x - start
                paths[key].append(f"M{start} {y}h{width}v1h-{width}z")
    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {GRID_SIZE} {GRID_SIZE}"'
        ' shape-rendering="crispEdges">'
    ]
    for key, color in PALETTE.items():
        if paths[key]:
            lines.append(f'  <path fill="{color}" d="{"".join(paths[key])}"/>')
    lines.append("</svg>")
    return "\n".join(lines) + "\n"


def box_weights(src, dst):
    scale = src / dst
    table = []
    for o in range(dst):
        lo, hi = o * scale, (o + 1) * scale
        taps = []
        for s in range(int(math.floor(lo)), min(int(math.ceil(hi)), src)):
            w = min(hi, s + 1) - max(lo, s)
            if w > 0:
                taps.append((s, w / scale))
        table.append(taps)
    return table


def resample(channels, src, dst):
    if src == dst:
        return channels
    weights = box_weights(src, dst)
    out = []
    for ch in channels:
        rows = []
        for y in range(src):
            base = y * src
            rows.append([sum(ch[base + s] * w for s, w in taps) for taps in weights])
        res = [0.0] * (dst * dst)
        for x in range(dst):
            for y, taps in enumerate(weights):
                res[y * dst + x] = sum(rows[s][x] * w for s, w in taps)
        out.append(res)
    return out


def to_rgba8(channels, size):
    r, g, b, a = channels
    buf = bytearray(size * size * 4)
    for i in range(size * size):
        alpha = a[i]
        if alpha <= 0.0:
            continue
        k = i * 4
        buf[k] = min(255, round(r[i] / alpha * 255))
        buf[k + 1] = min(255, round(g[i] / alpha * 255))
        buf[k + 2] = min(255, round(b[i] / alpha * 255))
        buf[k + 3] = min(255, round(alpha * 255))
    return bytes(buf)


def png_chunk(kind, data):
    body = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)


def encode_png(rgba, size):
    stride = size * 4
    raw = b"".join(b"\x00" + rgba[y * stride:(y + 1) * stride] for y in range(size))
    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", header)
        + png_chunk(b"IDAT", zlib.compress(raw, 9))
        + png_chunk(b"IEND", b"")
    )


def encode_ico(pngs):
    count = len(pngs)
    out = struct.pack("<HHH", 0, 1, count)
    offset = 6 + 16 * count
    for size, data in pngs:
        dim = 0 if size >= 256 else size
        out += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    return out + b"".join(data for _, data in pngs)


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    logo_dir = os.path.join(root, "assets", "logo")
    icon_dir = os.path.join(root, "assets", "icon")
    os.makedirs(logo_dir, exist_ok=True)
    os.makedirs(icon_dir, exist_ok=True)

    with open(os.path.join(logo_dir, "nebucat.svg"), "w", newline="\n") as f:
        f.write(render_svg())

    master = render_master()
    pngs = []
    for size in SIZES:
        rgba = to_rgba8(resample(master, GRID_SIZE, size), size)
        png = encode_png(rgba, size)
        pngs.append((size, png))
        with open(os.path.join(icon_dir, f"nebucat-{size}.png"), "wb") as f:
            f.write(png)
        if size == 32:
            with open(os.path.join(icon_dir, "nebucat-32.rgba"), "wb") as f:
                f.write(rgba)

    with open(os.path.join(icon_dir, "nebucat.ico"), "wb") as f:
        f.write(encode_ico(pngs))


if __name__ == "__main__":
    main()
