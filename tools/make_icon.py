#!/usr/bin/env python3
"""Render the placeholder nebucat lettermark into assets/icon/."""

import math
import os
import struct
import zlib

MASTER = 1024
SUB = 4
SIZES = (16, 24, 32, 48, 64, 256)
UNITS = 256.0

BACKGROUND = (0x15 / 255, 0x14 / 255, 0x1B / 255)
ACCENT = (0xB7 / 255, 0x9C / 255, 0xFF / 255)

SQUARE_RADIUS = 0.22 * UNITS
LEFT_STEM = (76.0, 72.0, 112.0, 184.0)
RIGHT_STEM = (144.0, 124.0, 180.0, 184.0)
ARCH_CENTER = (128.0, 124.0)
ARCH_OUTER = 52.0
ARCH_INNER = 16.0


def sd_box(x, y, x0, y0, x1, y1, r=0.0):
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    qx = abs(x - cx) - ((x1 - x0) / 2 - r)
    qy = abs(y - cy) - ((y1 - y0) / 2 - r)
    outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
    return outside + min(max(qx, qy), 0.0) - r


def sd_square(x, y):
    return sd_box(x, y, 0.0, 0.0, UNITS, UNITS, SQUARE_RADIUS)


def sd_letter(x, y):
    d = min(sd_box(x, y, *LEFT_STEM), sd_box(x, y, *RIGHT_STEM))
    dist = math.hypot(x - ARCH_CENTER[0], y - ARCH_CENTER[1])
    ring = max(dist - ARCH_OUTER, ARCH_INNER - dist)
    arch = max(ring, y - ARCH_CENTER[1])
    return min(d, arch)


def coverage(sdf, ux, uy, unit, edge, offsets):
    # SDFs here never overestimate distance, so the fast path is exact.
    d = sdf(ux, uy)
    if d >= edge:
        return 0.0
    if d <= -edge:
        return 1.0
    hits = 0
    for oy in offsets:
        for ox in offsets:
            if sdf(ux + ox * unit, uy + oy * unit) < 0.0:
                hits += 1
    return hits / (len(offsets) ** 2)


def render(size):
    unit = UNITS / size
    edge = 0.75 * unit
    offsets = [(i + 0.5) / SUB - 0.5 for i in range(SUB)]
    n = size * size
    r, g, b, a = [0.0] * n, [0.0] * n, [0.0] * n, [0.0] * n
    for py in range(size):
        uy = (py + 0.5) * unit
        letter_row = LEFT_STEM[1] - edge <= uy <= LEFT_STEM[3] + edge
        for px in range(size):
            ux = (px + 0.5) * unit
            cs = coverage(sd_square, ux, uy, unit, edge, offsets)
            if cs == 0.0:
                continue
            cn = coverage(sd_letter, ux, uy, unit, edge, offsets) if letter_row else 0.0
            cn = min(cn, cs)
            i = py * size + px
            r[i] = BACKGROUND[0] * (cs - cn) + ACCENT[0] * cn
            g[i] = BACKGROUND[1] * (cs - cn) + ACCENT[1] * cn
            b[i] = BACKGROUND[2] * (cs - cn) + ACCENT[2] * cn
            a[i] = cs
    return [r, g, b, a]


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


def downsample(channels, src, dst):
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
    out_dir = os.path.join(root, "assets", "icon")
    os.makedirs(out_dir, exist_ok=True)

    master = render(MASTER)
    pngs = []
    for size in SIZES:
        rgba = to_rgba8(downsample(master, MASTER, size), size)
        png = encode_png(rgba, size)
        pngs.append((size, png))
        with open(os.path.join(out_dir, f"nebucat-{size}.png"), "wb") as f:
            f.write(png)
        if size == 32:
            with open(os.path.join(out_dir, "nebucat-32.rgba"), "wb") as f:
                f.write(rgba)

    with open(os.path.join(out_dir, "nebucat.ico"), "wb") as f:
        f.write(encode_ico(pngs))


if __name__ == "__main__":
    main()
