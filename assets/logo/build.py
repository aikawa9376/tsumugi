"""Generate Tsumugi's weave mark and geometric wordmark using only Python."""
from pathlib import Path
import sys

TEAL = "#2aa198"
GOLD = "#d7a85c"
INK = "#182430"
PAPER = "#edf3f4"


def svg(width, height, body):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
            f'viewBox="0 0 {width} {height}" role="img" aria-label="Tsumugi">\n'
            '<title>Tsumugi</title>\n' + body + '\n</svg>\n')


def weave():
    # The alternating gaps show two threads crossing over and under the weft.
    return ('<g fill="none" stroke-linecap="round" stroke-width="9">'
            f'<path stroke="{TEAL}" d="M34 17v66M17 28h66M17 72h66"/>'
            f'<path stroke="{GOLD}" d="M66 17v22m0 22v22M17 50h66"/>'
            f'<path stroke="{TEAL}" d="M34 39v22"/>'
            '</g>')


# These letterforms are drawn as paths; the logo needs no installed font.
LETTERS = [
    (25, 'M10 3v31q0 8 9 8M2 15h21'),
    (29, 'M25 18c-5-5-21-5-21 3s21 3 21 12-16 12-23 5'),
    (30, 'M4 15v15q0 13 11 13t11-13V15'),
    (43, 'M4 42V16m0 9q0-10 9-10t9 10v17m0-17q0-10 9-10t9 10v17'),
    (30, 'M4 15v15q0 13 11 13t11-13V15'),
    (31, 'M26 17v29q0 13-21 8M26 28c0-17-23-17-23 0s23 17 23 0'),
    (13, 'M6 17v25M6 5h.1'),
]


def wordmark(fill):
    x = 0
    paths = []
    for width, path in LETTERS:
        paths.append(f'<path transform="translate({x} 0)" d="{path}"/>')
        x += width + 7
    return (f'<g fill="none" stroke="{fill}" stroke-width="4.5" '
            'stroke-linecap="round" stroke-linejoin="round">' + ''.join(paths) + '</g>')


def main(destination):
    destination.mkdir(parents=True, exist_ok=True)
    for light, fill in [(False, INK), (True, PAPER)]:
        suffix = '-light' if light else ''
        (destination / f'tsumugi-mark{suffix}.svg').write_text(svg(100, 100, weave()))
        for size in [16, 32]:
            (destination / f'tsumugi-mark-{size}{suffix}.svg').write_text(
                svg(size, size, f'<g transform="scale({size / 100})">{weave()}</g>'))
        (destination / f'tsumugi-logo-horizontal{suffix}.svg').write_text(
            svg(360, 90, '<g transform="translate(0 -5)">' + weave() + '</g>'
                + '<g transform="translate(112 13)">' + wordmark(fill) + '</g>'))
        (destination / f'tsumugi-logo-vertical{suffix}.svg').write_text(
            svg(260, 180, '<g transform="translate(80 0)">' + weave() + '</g>'
                + '<g transform="translate(8 112)">' + wordmark(fill) + '</g>'))
    (destination / 'tsumugi-icon.svg').write_text(svg(128, 128,
        f'<rect width="128" height="128" rx="28" fill="{INK}"/>'
        + '<g transform="translate(9 9) scale(1.1)">' + weave() + '</g>'))


if __name__ == '__main__':
    main(Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent)
