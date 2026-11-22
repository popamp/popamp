"""Draws the Popamp logo: a brushed-metal knob in a red neon ring with a lightning bolt cut into it.

Writes popamp.svg and popamp-small.svg (a plain yellow knob for 16 and 24 px)
next to this script, then renders png/popamp-N.png with inkscape and packs
them into popamp.ico for Windows (the program, its shortcuts and installer)
and popamp.icns for macOS (Popamp.app in the Dock and Finder).

Usage: python3 assets/logo/make-logo.py
"""
import math
import struct
import subprocess
from pathlib import Path

RING = "#ff3d6e"
BOLT_CORE = "#fff3c4"
BOLT_HALO = "#ffd23f"
CX = CY = 128
SCALE = 1.1  # knob size in the 256 tile

# Bolt outline, clockwise from the top-left corner (units of the reference drawing).
BOLT = [(487, 316), (578, 316), (515, 422), (598, 422), (462, 635), (505, 485), (432, 485)]

BIG_SIZES = (32, 48, 64, 128, 256, 512, 1024)
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)
SMALL_SIZES = (16, 24)

TILE = """<linearGradient id="tile" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#343842"/><stop offset="1" stop-color="#1d2027"/></linearGradient>"""
HEAD = '<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256"><title>Popamp</title><defs>\n'


def bolt_points(height: float) -> str:
    xs, ys = [x for x, _ in BOLT], [y for _, y in BOLT]
    k = height / (max(ys) - min(ys))
    bx, by = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    return " ".join(f"{CX + (x - bx) * k:.2f},{CY + (y - by) * k:.2f}" for x, y in BOLT)


def brushed(r: float, sectors: int = 180) -> str:
    """Conic brushed-metal shading: thin wedges whose grey level runs round the disc, plus faint lathe rings."""
    out = []
    for i in range(sectors):
        a0, a1 = 2 * math.pi * i / sectors, 2 * math.pi * (i + 1.6) / sectors
        mid = (a0 + a1) / 2
        # light at top and bottom, dark at the sides, with a faint finer ripple
        lum = 0.70 + 0.20 * math.cos(2 * (mid - math.pi / 2) + 0.35) + 0.05 * math.cos(6 * mid + 1.0)
        g = max(0, min(255, round(255 * lum)))
        x0, y0 = CX + r * math.cos(a0), CY + r * math.sin(a0)
        x1, y1 = CX + r * math.cos(a1), CY + r * math.sin(a1)
        out.append(f'<path d="M{CX} {CY}L{x0:.2f} {y0:.2f}A{r} {r} 0 0 1 {x1:.2f} {y1:.2f}Z" fill="rgb({g},{g},{g + 4})"/>')
    rings = "".join(f'<circle cx="{CX}" cy="{CY}" r="{r * t / 60:.1f}" fill="none" stroke="{"#fff" if t % 2 else "#000"}" '
                    f'stroke-opacity="0.05" stroke-width="0.8"/>' for t in range(4, 60))
    return f'<g filter="url(#smooth)">{"".join(out)}</g>{rings}'


def big_svg() -> str:
    r_ring, r_disc = 92 * SCALE, 80 * SCALE
    pts = bolt_points(108 * SCALE)
    edge = f'fill="none" stroke="{BOLT_HALO}" stroke-linejoin="miter" stroke-miterlimit="12"'
    return HEAD + f"""{TILE}
<radialGradient id="pit" cx="0.5" cy="0.35" r="0.8"><stop offset="0" stop-color="#3a1d63"/><stop offset="1" stop-color="#140a26"/></radialGradient>
<filter id="glow" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="5"/></filter>
<filter id="glow2" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="2.2"/></filter>
<filter id="smooth"><feGaussianBlur stdDeviation="0.6"/></filter>
<filter id="shadow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="4"/></filter>
<clipPath id="disc"><circle cx="{CX}" cy="{CY}" r="{r_disc}"/></clipPath></defs>
<rect width="256" height="256" rx="56" fill="url(#tile)"/>
<rect x="1" y="1" width="254" height="254" rx="55" fill="none" stroke="#fff" stroke-opacity="0.10" stroke-width="2"/>
<circle cx="{CX}" cy="{CY}" r="{r_ring}" fill="none" stroke="{RING}" stroke-width="{9 * SCALE:.1f}" filter="url(#glow)" opacity="0.9"/>
<circle cx="{CX}" cy="{CY}" r="{r_ring}" fill="none" stroke="{RING}" stroke-width="{4 * SCALE:.1f}"/>
<circle cx="{CX}" cy="{CY}" r="{r_ring - 2.5 * SCALE}" fill="none" stroke="#fff" stroke-opacity="0.35" stroke-width="1"/>
<circle cx="{CX}" cy="{CY + 3}" r="{r_disc + 1}" fill="#000" opacity="0.6" filter="url(#shadow)"/>
<g clip-path="url(#disc)">{brushed(r_disc + 1)}</g>
<circle cx="{CX}" cy="{CY}" r="{r_disc - 1}" fill="none" stroke="#fff" stroke-opacity="0.55" stroke-width="1.5"/>
<circle cx="{CX}" cy="{CY}" r="{r_disc - 0.5}" fill="none" stroke="#000" stroke-opacity="0.35" stroke-width="1"/>
<polygon points="{pts}" {edge} stroke-width="{8 * SCALE:.1f}" filter="url(#glow)"/>
<polygon points="{pts}" fill="url(#pit)"/>
<polygon points="{pts}" {edge} stroke-width="{4.5 * SCALE:.1f}" filter="url(#glow2)"/>
<polygon points="{pts}" {edge} stroke-width="{3.2 * SCALE:.1f}"/>
<polygon points="{pts}" {edge.replace(BOLT_HALO, BOLT_CORE)} stroke-width="{1.4 * SCALE:.1f}"/>
</svg>
"""


def small_svg() -> str:
    """The knob for tiny sizes, where the bolt would be a smudge: a yellow disc in the red ring."""
    return HEAD + f"""{TILE}
<radialGradient id="sun" cx="0.45" cy="0.38" r="0.7"><stop offset="0" stop-color="{BOLT_CORE}"/><stop offset="0.45" stop-color="{BOLT_HALO}"/><stop offset="1" stop-color="#f0a800"/></radialGradient>
<filter id="glow" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="6"/></filter></defs>
<rect width="256" height="256" rx="56" fill="url(#tile)"/>
<circle cx="{CX}" cy="{CY}" r="92" fill="none" stroke="{RING}" stroke-width="28.8" filter="url(#glow)" opacity="0.9"/>
<circle cx="{CX}" cy="{CY}" r="92" fill="none" stroke="{RING}" stroke-width="16"/>
<circle cx="{CX}" cy="{CY}" r="70" fill="url(#sun)"/></svg>
"""


def main() -> None:
    here = Path(__file__).resolve().parent
    jobs = [("popamp.svg", big_svg(), BIG_SIZES), ("popamp-small.svg", small_svg(), SMALL_SIZES)]
    (here / "png").mkdir(exist_ok=True)
    for name, svg, sizes in jobs:
        (here / name).write_text(svg)
        for n in sizes:
            subprocess.run(["inkscape", str(here / name), "-w", str(n), "-h", str(n), "-o", str(here / "png" / f"popamp-{n}.png")],
                           check=True, capture_output=True)
    png = lambda n: (here / "png" / f"popamp-{n}.png").read_bytes()
    write_ico(here / "popamp.ico", [(n, png(n)) for n in ICO_SIZES])
    # macOS's icon slots: each type stands for one size (the @2x ones twice).
    slots = [("icp4", 16), ("icp5", 32), ("ic11", 32), ("ic12", 64), ("ic07", 128), ("ic13", 256), ("ic08", 256), ("ic14", 512), ("ic09", 512), ("ic10", 1024)]
    write_icns(here / "popamp.icns", [(kind, png(n)) for kind, n in slots])


def write_ico(path: Path, images: list[tuple[int, bytes]]) -> None:
    """An .ico of PNG images, each (size, data): a header, one directory entry each, then the data."""
    head = struct.pack("<HHH", 0, 1, len(images))
    offset = len(head) + 16 * len(images)
    entries, data = b"", b""
    for n, png in images:
        side = n % 256  # 0 stands for 256
        entries += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(png), offset + len(data))
        data += png
    path.write_bytes(head + entries + data)


def write_icns(path: Path, images: list[tuple[str, bytes]]) -> None:
    """An .icns of PNG images, each (type, data): the header, then each image with its type and length."""
    body = b"".join(kind.encode() + struct.pack(">I", 8 + len(png)) + png for kind, png in images)
    path.write_bytes(b"icns" + struct.pack(">I", 8 + len(body)) + body)


if __name__ == "__main__":
    main()
