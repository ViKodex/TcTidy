"""Generate the TcTidy logo SVGs on a clean 2:1 dimetric grid.

    python assets/logo/generate.py --font Poppins-Medium.ttf

needs fontTools and uharfbuzz (pip install fonttools uharfbuzz) and the tagline
font: Poppins Medium, SIL Open Font License, from https://github.com/google/fonts
(ofl/poppins). This script is the editable master; the SVGs it writes are the
published artwork, with every letter outlined so no font is needed to view them.

It writes, next to itself, each logo twice with fixed colours, for light and for
dark backgrounds (a README picks one with <picture>):

    tctidy-logo.svg / tctidy-logo-dark.svg   cube, wordmark and tagline
    tctidy-mark.svg / tctidy-mark-dark.svg   the detailed cube, 48 px and up
    tctidy-icon.svg / tctidy-icon-dark.svg   a simplified cube for 16 to 32 px

and the settings page's favicon (crates/tctidy-cli/src/settings_page/icon.svg),
the one file that switches colour itself, with prefers-color-scheme, as an
adaptive favicon must. The page's app bar inlines the small icon in index.html:
update it by hand after changing SMALL.

Cube: top rhombus T(64,8) R(120,36) B(64,64) L(8,36), vertical edges 64 long.
Faces are separated by transparent seams; details (braces, code lines) are cut
out with a mask so they show the background. The wordmark is custom lettering
built from rectangles, arcs and polygons; the tagline is outlined from the font.
"""
import argparse
import math
from pathlib import Path

INK = "#1c2c3d"
INK_DARK = "#e6edf3"
ORANGE = "#f88a24"


def f(v):
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s == "-0" else s


def pts(points):
    return " ".join(f"{f(x)},{f(y)}" for x, y in points)


def offset_polygon(poly, offsets):
    """Move each edge i (poly[i] -> poly[i+1]) inward by offsets[i]; return the
    new vertices (clockwise polygon in screen coordinates)."""
    n = len(poly)
    lines = []
    for i in range(n):
        (x1, y1), (x2, y2) = poly[i], poly[(i + 1) % n]
        dx, dy = x2 - x1, y2 - y1
        length = math.hypot(dx, dy)
        # Inward normal of a clockwise polygon in y-down coordinates.
        nx, ny = -dy / length, dx / length
        d = offsets[i]
        lines.append(((x1 + nx * d, y1 + ny * d), (dx, dy)))
    out = []
    for i in range(n):
        (p, r), (q, s) = lines[i - 1], lines[i]
        cross = r[0] * s[1] - r[1] * s[0]
        t = ((q[0] - p[0]) * s[1] - (q[1] - p[1]) * s[0]) / cross
        out.append((p[0] + r[0] * t, p[1] + r[1] * t))
    return out


T, R, B, L = (64, 8), (120, 36), (64, 64), (8, 36)
LB, BB, RB = (8, 100), (64, 128), (120, 100)


def faces(gap, radius):
    seam = gap / 2 + radius
    top = offset_polygon([T, R, B, L], [radius, seam, seam, radius])
    left = offset_polygon([L, B, BB, LB], [seam, seam, radius, radius])
    right = offset_polygon([B, R, RB, BB], [seam, radius, radius, seam])
    return [top, left, right]


def left_face(u, v):
    return (8 + u, 36 + 0.5 * u + v)


def right_face(u, v):
    return (64 + u, 64 - 0.5 * u + v)


def brace(cu, cv, h, w, mirrored=False):
    """A '{' (or '}' when mirrored) centred on (cu, cv) of the left face, h tall
    and w deep in face units, drawn on the face: every point, control points
    included, goes through the face's mapping, so the curves stay exact and the
    stroke keeps its width."""
    a = h / 2
    k = h * 0.14
    tip = -w / 2
    spine = -w / 2 + w * 0.35
    end = w / 2

    def at(du, dv):
        du = -du if mirrored else du
        x, y = left_face(cu + du, cv + dv)
        return f"{f(x)} {f(y)}"

    return (
        f"M{at(end, -a)}"
        f"Q{at(spine, -a)} {at(spine, -a + k)}"
        f"L{at(spine, -k)}"
        f"Q{at(spine, 0)} {at(tip, 0)}"
        f"Q{at(spine, 0)} {at(spine, k)}"
        f"L{at(spine, a - k)}"
        f"Q{at(spine, a)} {at(end, a)}"
    )


def centroid(poly):
    area = cx = cy = 0
    for (x1, y1), (x2, y2) in zip(poly, poly[1:] + poly[:1]):
        cross = x1 * y2 - x2 * y1
        area += cross
        cx += (x1 + x2) * cross
        cy += (y1 + y2) * cross
    return cx / (3 * area), cy / (3 * area)


def visible_centres(gap):
    """Centres, in face units (u, v), of what shows of the left and right faces:
    each face less half a seam along its two seam edges."""
    half = gap / 2
    lx, ly = centroid(offset_polygon([L, B, BB, LB], [half, half, 0, 0]))
    rx, ry = centroid(offset_polygon([B, R, RB, BB], [half, 0, 0, half]))
    left_u, right_u = lx - 8, rx - 64
    return (left_u, ly - 36 - 0.5 * left_u), (right_u, ry - 64 + 0.5 * right_u)


def cube(spec, prefix):
    """Faces, mask and orange accents for one cube spec. The braces and the block
    of code lines are centred on what shows of their faces."""
    gap, radius = spec["gap"], spec["radius"]
    sw = spec["stroke"]
    (lu, lv), (ru, rv) = visible_centres(gap)
    h, w, dx = spec["brace_h"], spec["brace_w"], spec["brace_dx"]
    cut = [
        f'<path d="{brace(lu - dx, lv, h, w)}"/>',
        f'<path d="{brace(lu + dx, lv, h, w, mirrored=True)}"/>',
    ]
    rows = spec["rows"]
    us = [u for _, segments in rows for (u0, u1, _) in segments for u in (u0, u1)]
    vs = [v for v, _ in rows]
    shift_u = ru - (min(us) + max(us)) / 2
    shift_v = rv - (min(vs) + max(vs)) / 2
    accents = []
    for v, segments in rows:
        for (u0, u1, kind) in segments:
            (x0, y0) = right_face(u0 + shift_u, v + shift_v)
            (x1, y1) = right_face(u1 + shift_u, v + shift_v)
            line = f'x1="{f(x0)}" y1="{f(y0)}" x2="{f(x1)}" y2="{f(y1)}"'
            if kind == "o":
                accents.append(f"<line {line}/>")
            else:
                cut.append(f"<line {line}/>")
    face_polys = "".join(f'<polygon points="{pts(p)}"/>' for p in faces(gap, radius))
    return f"""<mask id="{prefix}-cut" maskUnits="userSpaceOnUse" x="0" y="0" width="128" height="136">
    <rect width="128" height="136" fill="#fff"/>
    <g fill="none" stroke="#000" stroke-width="{f(sw)}" stroke-linecap="round" stroke-linejoin="round">{''.join(cut)}</g>
  </mask>""", f"""<g class="ink" mask="url(#{prefix}-cut)" stroke-width="{f(2 * radius)}" stroke-linejoin="round">{face_polys}</g>
  <g class="accent" fill="none" stroke-width="{f(sw)}" stroke-linecap="round">{''.join(accents)}</g>"""


# The right face shows formatted code: an IF block whose body is indented one
# level, then one more statement. Rows are (v, [(u_start, u_end, "w" or "o")]) in
# face units; lines of a level start on the same u, and the block is centred on
# the visible face.
#
#     IF bStart AND bReady THEN
#         nCount := nCount + 1;
#         bBusy := TRUE;              (orange)
#         fSpeed := fTarget * fScale;
#     END_IF
#     bDone := NOT bBusy;

# Full-detail cube: the six lines above.
DETAIL = {
    "gap": 4,
    "radius": 2.5,
    "stroke": 3.2,
    "brace_dx": 9,
    "brace_h": 30,
    "brace_w": 8,
    "rows": [
        (15, [(9, 36, "w")]),
        (22.4, [(16, 40, "w")]),
        (29.8, [(16, 33, "o")]),
        (37.2, [(16, 44, "w")]),
        (44.6, [(9, 22, "w")]),
        (52, [(9, 30, "w")]),
    ],
}

# Small sizes (16 to 32 px): fewer, thicker marks. Three lines: IF, an indented
# body line, END_IF.
SMALL = {
    "gap": 6,
    "radius": 3,
    "stroke": 7,
    "brace_dx": 12.5,
    "brace_h": 40,
    "brace_w": 10,
    "rows": [
        (18, [(12, 40, "w")]),
        (32, [(20, 44, "o")]),
        (46, [(12, 28, "w")]),
    ],
}

LIGHT_BG = {"ink": INK, "accent": ORANGE}
DARK_BG = {"ink": INK_DARK, "accent": ORANGE}


def style(colors, adaptive=False):
    rules = (
        f".ink {{ fill: {colors['ink']}; stroke: {colors['ink']}; }}"
        f" .text {{ fill: {colors['ink']}; }}"
        f" .accent {{ stroke: {colors['accent']}; }}"
    )
    if adaptive:
        rules += (
            f" @media (prefers-color-scheme: dark) {{ .ink {{ fill: {INK_DARK}; stroke: {INK_DARK}; }}"
            f" .text {{ fill: {INK_DARK}; }} }}"
        )
    return f"<style>{rules}</style>"


def svg(view_box, title, body, defs, css):
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}" role="img" aria-labelledby="title">
  <title id="title">{title}</title>
  {css}
  <defs>
  {defs}
  </defs>
  {body}
</svg>
"""


# ---- wordmark: geometric letters on a 59-unit cap height, 9.5 stroke ----------------

CAP_TOP, BASE, X_TOP, DESC = 34, 93, 50, 109
SW = 9.5


def letter_t(x0, width=46):
    stem = x0 + width / 2 - SW / 2
    return (
        f'<rect x="{f(x0)}" y="{CAP_TOP}" width="{f(width)}" height="{SW}"/>'
        f'<rect x="{f(stem)}" y="{CAP_TOP}" width="{SW}" height="{BASE - CAP_TOP}"/>'
    )


def bowl_radius():
    return (BASE - X_TOP) / 2 - SW / 2


def letter_c(x0, open_deg=50):
    r = bowl_radius()
    cx, cy = x0 + (BASE - X_TOP) / 2, (BASE + X_TOP) / 2
    a = math.radians(open_deg)
    sx, sy = cx + r * math.cos(a), cy - r * math.sin(a)
    ex, ey = cx + r * math.cos(a), cy + r * math.sin(a)
    return (
        f'<path d="M{f(sx)} {f(sy)}A{f(r)} {f(r)} 0 1 0 {f(ex)} {f(ey)}" '
        f'fill="none" stroke-width="{SW}"/>'
    )


def letter_i(x0):
    cx = x0 + SW / 2
    return (
        f'<rect x="{f(x0)}" y="{X_TOP}" width="{SW}" height="{BASE - X_TOP}"/>'
        f'<circle cx="{f(cx)}" cy="{f(CAP_TOP + 5.2)}" r="5.2"/>'
    )


def letter_d(x0):
    r = bowl_radius()
    outer = (BASE - X_TOP) / 2
    stem_x = x0 + 2 * outer - SW
    cx, cy = x0 + outer, (BASE + X_TOP) / 2
    return (
        f'<circle cx="{f(cx)}" cy="{f(cy)}" r="{f(r)}" fill="none" stroke-width="{SW}"/>'
        f'<rect x="{f(stem_x)}" y="{CAP_TOP}" width="{SW}" height="{BASE - CAP_TOP}"/>'
    )


def letter_y(x0, width=34, slope=0.42):
    """Two straight arms with equal slopes: the left one ends where it meets the
    right one, which runs on to the descender."""
    left_top = x0 + 5.1
    right_top = x0 + width - 5.1
    meet = X_TOP + (right_top - left_top) / (2 * slope)
    join_x = left_top + slope * (meet - X_TOP)
    hw = (SW / 2) * math.sqrt(1 + slope * slope)
    tail = right_top - slope * (DESC - X_TOP)
    left_arm = [(left_top - hw, X_TOP), (left_top + hw, X_TOP), (join_x + hw, meet), (join_x - hw, meet)]
    right_arm = [(right_top - hw, X_TOP), (right_top + hw, X_TOP), (tail + hw, DESC), (tail - hw, DESC)]
    return f'<polygon points="{pts(left_arm)}"/><polygon points="{pts(right_arm)}"/>'


def wordmark():
    x = 141.7
    parts = []
    parts.append(letter_t(x)); x += 46 - 0.6        # c tucks under the T bar
    parts.append(letter_c(x)); x += 42.3
    parts.append(letter_t(x)); x += 46 + 6.8
    parts.append(letter_i(x)); x += SW + 8.5
    parts.append(letter_d(x)); x += (BASE - X_TOP) + 7.6
    parts.append(letter_y(x))
    end = x + 34
    return "".join(parts), end


# ---- tagline: outlined from the font -----------------------------------------------

TAGLINE = "Rust formatter for Structured Text"
TAGLINE_CAP = 9.4       # cap height, in logo units
TAGLINE_BASE = 125
TAGLINE_WEIGHT = 500
DASH = 11               # orange dash length at each end
DASH_GAP = 7            # space between a dash and the text


def outline_text(font_path, text, weight, cap_height, x_center, baseline, max_width):
    """`text` shaped with HarfBuzz (kerning included) and drawn as one path,
    centred on x_center, scaled to cap_height (or narrower, to fit max_width)."""
    import uharfbuzz as hb
    from fontTools.pens.svgPathPen import SVGPathPen
    from fontTools.pens.transformPen import TransformPen
    from fontTools.ttLib import TTFont

    font = TTFont(font_path)
    location = {"wght": weight} if "fvar" in font else None
    glyphs = font.getGlyphSet(location=location)
    upem = font["head"].unitsPerEm
    cap = getattr(font["OS/2"], "sCapHeight", 0) or 0.7 * upem

    blob = hb.Blob.from_file_path(str(font_path))
    hb_font = hb.Font(hb.Face(blob))
    if location:
        hb_font.set_variations(location)
    buf = hb.Buffer()
    buf.add_str(text)
    buf.guess_segment_properties()
    hb.shape(hb_font, buf, {"kern": True, "liga": True})
    advance = sum(p.x_advance for p in buf.glyph_positions)

    scale = cap_height / cap
    if advance * scale > max_width:
        scale = max_width / advance
    width = advance * scale
    order = font.getGlyphOrder()
    pen = SVGPathPen(glyphs, ntos=f)
    x = x_center - width / 2
    for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
        glyph = glyphs[order[info.codepoint]]
        dx = x + pos.x_offset * scale
        dy = baseline - pos.y_offset * scale
        glyph.draw(TransformPen(pen, (scale, 0, 0, -scale, dx, dy)))
        x += pos.x_advance * scale
    return pen.getCommands(), width


def logo(font_path, colors):
    defs, body = cube(DETAIL, "l")
    letters, end = wordmark()
    left, right = 141.7, end
    center = (left + right) / 2
    room = right - left - 2 * (DASH + DASH_GAP)
    path, width = outline_text(font_path, TAGLINE, TAGLINE_WEIGHT, TAGLINE_CAP,
                               center, TAGLINE_BASE, room)
    dash_y = TAGLINE_BASE - TAGLINE_CAP / 2
    text_left, text_right = center - width / 2, center + width / 2
    tagline = (
        f'<g class="accent" stroke-width="1.6">'
        f'<line x1="{f(text_left - DASH_GAP - DASH)}" y1="{f(dash_y)}" x2="{f(text_left - DASH_GAP)}" y2="{f(dash_y)}"/>'
        f'<line x1="{f(text_right + DASH_GAP)}" y1="{f(dash_y)}" x2="{f(text_right + DASH_GAP + DASH)}" y2="{f(dash_y)}"/>'
        f'</g>'
        f'<path class="text" d="{path}"/>'
    )
    word = f'<g class="ink" stroke-width="0">{letters}</g>{tagline}'
    view = f"0 0 {math.ceil(end + 8)} 136"
    return svg(view, "TcTidy: Rust formatter for Structured Text", body + "\n  " + word, defs, style(colors))


def write(path, text):
    path.write_text(text, encoding="utf-8", newline="\n")


def main():
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--font", required=True, type=Path, help="tagline font (Poppins Medium)")
    parser.add_argument("--out", type=Path, default=here, help="output directory")
    parser.add_argument("--favicon", type=Path,
                        default=here.parent.parent / "crates/tctidy-cli/src/settings_page/icon.svg")
    args = parser.parse_args()
    out = args.out
    out.mkdir(parents=True, exist_ok=True)

    for suffix, colors in (("", LIGHT_BG), ("-dark", DARK_BG)):
        defs, body = cube(DETAIL, "m")
        write(out / f"tctidy-mark{suffix}.svg", svg("0 0 128 136", "TcTidy", body, defs, style(colors)))
        defs, body = cube(SMALL, "s")
        write(out / f"tctidy-icon{suffix}.svg", svg("0 0 128 136", "TcTidy", body, defs, style(colors)))
        write(out / f"tctidy-logo{suffix}.svg", logo(args.font, colors))

    defs, body = cube(SMALL, "s")
    write(args.favicon, svg("0 0 128 136", "TcTidy", body, defs, style(LIGHT_BG, adaptive=True)))


if __name__ == "__main__":
    main()
