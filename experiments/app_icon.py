"""Generates data/icons/com.github.xierongchuan.karpender.svg.

Issue 1 asks for a logo in the style of recent macOS. Two things define that
style, and the icon this replaces had neither:

1. The outline is a squircle, not a circle. macOS rounds an app icon with a
   superellipse, which keeps a genuinely straight run along the middle of every
   edge and puts all the curvature in the corners. The old icon used a rounded
   rectangle whose corners started 47 percent of the way in from each edge, so
   it read as a slightly squashed circle.
2. The artwork sits on a fixed grid. Apple insets the shape by roughly ten
   percent of the canvas, which is the room the system shadow and the hover
   scaling need, and centres a single glyph inside it. The old icon filled the
   canvas edge to edge and carried two competing objects, a microphone and a
   shield, which turn to mush below 32 pixels.

The squircle is emitted as a Catmull-Rom fit of the superellipse
``|x/a|^5 + |y/a|^5 = 1``. Running this file prints the worst deviation of the
fit from the true curve so the sample count stays honest.

Run with: python3 experiments/app_icon.py
"""

import math
from pathlib import Path

CANVAS = 1024
# Ten percent inset on every side, the same proportion as the macOS icon grid.
HALF = (CANVAS - 2 * 102) / 2
CENTRE = CANVAS / 2
EXPONENT = 5.0
SAMPLES = 48
# Close enough for the sampling weight; the exact value only balances turning
# angle against distance travelled.
PERIMETER = 4 * 2 * HALF

OUTPUT = Path(__file__).resolve().parent.parent / "data/icons/com.github.xierongchuan.karpender.svg"


def superellipse(t):
    """Point at parameter `t` on the squircle, in canvas coordinates."""
    cos, sin = math.cos(t), math.sin(t)
    x = math.copysign(abs(cos) ** (2.0 / EXPONENT), cos)
    y = math.copysign(abs(sin) ** (2.0 / EXPONENT), sin)

    return CENTRE + HALF * x, CENTRE + HALF * y


def catmull_rom(points):
    """Closed cubic path through `points`, tangents from the neighbours."""
    count = len(points)
    parts = [f"M{points[0][0]:.2f} {points[0][1]:.2f}"]
    for index in range(count):
        previous = points[(index - 1) % count]
        start = points[index]
        end = points[(index + 1) % count]
        following = points[(index + 2) % count]
        first = (start[0] + (end[0] - previous[0]) / 6, start[1] + (end[1] - previous[1]) / 6)
        second = (end[0] - (following[0] - start[0]) / 6, end[1] - (following[1] - start[1]) / 6)
        parts.append(
            f"C{first[0]:.2f} {first[1]:.2f} {second[0]:.2f} {second[1]:.2f}"
            f" {end[0]:.2f} {end[1]:.2f}"
        )
    parts.append("Z")

    return "".join(parts)


def reference(tolerance=0.25):
    """Dense polyline on the true curve, refined until no gap exceeds `tolerance`.

    The `t` parametrisation is singular at the four points where the curve
    meets an axis: with an exponent of 5 the coordinate leaves the axis like
    `t ** 0.4`, so a step of a thousandth of a radian still jumps twenty pixels.
    Bisecting whatever is still too coarse sidesteps that entirely.
    """
    parameters = [2 * math.pi * i / 256 for i in range(257)]
    while True:
        refined = [parameters[0]]
        split = False
        for left, right in zip(parameters, parameters[1:]):
            first, second = superellipse(left), superellipse(right)
            if math.dist(first, second) > tolerance:
                refined.append((left + right) / 2)
                split = True
            refined.append(right)
        parameters = refined
        if not split:
            return [superellipse(t) for t in parameters]


def resampled(points, count):
    """`count` points spread evenly over turning angle plus a little arc length.

    Evenly spaced arc length wastes samples on the straight middle of an edge
    and starves the corners, which is where all the curvature is. Evenly spaced
    turning angle does the opposite and leaves an edge with almost no samples.
    Adding a fraction of the arc length to the measure keeps both honest.
    """
    measure = [0.0]
    for index in range(1, len(points)):
        before = points[index - 1]
        current = points[index]
        step = math.dist(before, current)
        after = points[(index + 1) % len(points)]
        first = math.atan2(current[1] - before[1], current[0] - before[0])
        second = math.atan2(after[1] - current[1], after[0] - current[0])
        turn = abs((second - first + math.pi) % (2 * math.pi) - math.pi)
        measure.append(measure[-1] + turn + 0.45 * step * 2 * math.pi / PERIMETER)

    total = measure[-1]
    picked = []
    cursor = 0
    for index in range(count):
        target = total * index / count
        while cursor + 1 < len(measure) and measure[cursor + 1] < target:
            cursor += 1
        picked.append(points[cursor])

    return picked


def squircle_path(points):
    return catmull_rom(resampled(points, SAMPLES))


def fit_error(path_points, points):
    """Worst distance from the emitted path to the true curve, in pixels."""
    return max(min(math.dist(sample, truth) for truth in points) for sample in path_points)


def flatten(path, steps=24):
    """Points along the cubics of `path`, for the error measurement."""
    numbers = [float(word) for word in path.replace("M", " ").replace("C", " ").replace("Z", " ").split()]
    coordinates = list(zip(numbers[0::2], numbers[1::2]))
    samples = []
    start = coordinates[0]
    for index in range(1, len(coordinates), 3):
        first, second, end = coordinates[index : index + 3]
        for step in range(steps + 1):
            t = step / steps
            u = 1 - t
            samples.append(
                (
                    u**3 * start[0] + 3 * u * u * t * first[0] + 3 * u * t * t * second[0] + t**3 * end[0],
                    u**3 * start[1] + 3 * u * u * t * first[1] + 3 * u * t * t * second[1] + t**3 * end[1],
                )
            )
        start = end

    return samples


TEMPLATE = """<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <!-- Generated by experiments/app_icon.py, do not edit by hand. -->
  <defs>
    <linearGradient id="surface" x1="212" y1="102" x2="812" y2="922" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#5ecdf7"/>
      <stop offset="0.38" stop-color="#3f63f2"/>
      <stop offset="0.74" stop-color="#3226b4"/>
      <stop offset="1" stop-color="#1d1268"/>
    </linearGradient>
    <radialGradient id="key-light" cx="0.3" cy="0.16" r="0.78">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.42"/>
      <stop offset="0.55" stop-color="#ffffff" stop-opacity="0.08"/>
      <stop offset="1" stop-color="#ffffff" stop-opacity="0"/>
    </radialGradient>
    <linearGradient id="bevel" x1="512" y1="102" x2="512" y2="922" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#ffffff" stop-opacity="0.55"/>
      <stop offset="0.34" stop-color="#ffffff" stop-opacity="0.12"/>
      <stop offset="0.68" stop-color="#0b0733" stop-opacity="0.10"/>
      <stop offset="1" stop-color="#0b0733" stop-opacity="0.38"/>
    </linearGradient>
    <linearGradient id="glyph" x1="512" y1="268" x2="512" y2="836" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#ffffff"/>
      <stop offset="1" stop-color="#d9e6ff"/>
    </linearGradient>
    <filter id="cast-shadow" x="-25%" y="-25%" width="150%" height="150%">
      <feDropShadow dx="0" dy="20" stdDeviation="22" flood-color="#0a0b2e" flood-opacity="0.32"/>
    </filter>
    <clipPath id="body">
      <path d="{squircle}"/>
    </clipPath>
  </defs>

  <g filter="url(#cast-shadow)">
    <path d="{squircle}" fill="url(#surface)"/>
  </g>
  <g clip-path="url(#body)">
    <rect x="102" y="102" width="820" height="820" fill="url(#key-light)"/>
    <!-- Specular sweep across the upper third, the highlight a glass surface
         picks up from a light above and behind the viewer. -->
    <path d="M102 452C232 322 372 262 522 262C672 262 802 312 922 412V102H102Z"
          fill="#ffffff" opacity="0.10"/>
  </g>
  <path d="{squircle}" fill="none" stroke="url(#bevel)" stroke-width="3"/>

  <!-- One glyph only, so it still reads at 16 pixels: a microphone capsule in
       its cradle. The voice arcs to either side are deliberately faint and
       simply fade out at small sizes instead of smearing into the capsule. -->
  <g stroke="url(#glyph)" fill="none" stroke-linecap="round" stroke-linejoin="round">
    <path d="M330 452C312 490 312 542 330 580M694 452C712 490 712 542 694 580"
          stroke-width="22" opacity="0.42"/>
    <path d="M258 404C226 470 226 562 258 628M766 404C798 470 798 562 766 628"
          stroke-width="22" opacity="0.24"/>
    <path d="M389 470V520C389 588 444 643 512 643C580 643 635 588 635 520V470"
          stroke-width="38"/>
    <path d="M512 643V752M424 752H600" stroke-width="38"/>
  </g>
  <rect x="435" y="243" width="154" height="330" rx="77" fill="url(#glyph)"/>
  <path d="M435 320C435 277 469 243 512 243C555 243 589 277 589 320V352H435Z"
        fill="#ffffff" opacity="0.5"/>
  <path d="M470 372H554M470 442H554" stroke="#2f4fc4" stroke-width="22"
        stroke-linecap="round" opacity="0.5"/>
</svg>
"""


def main():
    points = reference()
    path = squircle_path(points)
    OUTPUT.write_text(TEMPLATE.format(squircle=path), encoding="utf-8")
    print(f"wrote {OUTPUT} ({len(path)} byte outline, {len(points)} reference points)")
    error = fit_error(flatten(path), points)
    print(f"worst deviation from the true superellipse: {error:.3f} px of 1024")


if __name__ == "__main__":
    main()
