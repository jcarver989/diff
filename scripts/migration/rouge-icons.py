#!/usr/bin/env python3
import re
from pathlib import Path
from xml.etree import ElementTree as ET

NUMBER = re.compile(r'[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?')


def absolute(d):
    d = d.strip()
    if not d.startswith('m'):
        return d
    x, y = list(NUMBER.finditer(d, 1))[:2]
    tail = d[y.end():].lstrip(' ,')
    move = f'M{x.group()} {y.group()}'
    if tail and tail[0] in '-+.0123456789':
        return f'{move}l{tail}'
    return move + tail


root = Path(__file__).resolve().parents[2] / 'crates/ui/rouge'
for source in sorted((root / 'assets/icons').glob('*.svg')):
    svg = ET.parse(source).getroot()
    assert svg.attrib['viewBox'] == '0 0 24 24'
    data = []
    for element in svg:
        tag = element.tag.split('}')[-1]
        attr = element.attrib
        if tag == 'path':
            data.append(absolute(attr['d']))
        elif tag == 'line':
            data.append('M{} {}L{} {}'.format(*(attr[k] for k in ('x1', 'y1', 'x2', 'y2'))))
        elif tag == 'circle':
            x, y, r = (float(attr[k]) for k in ('cx', 'cy', 'r'))
            data.append(f'M{x-r:g} {y:g}a{r:g} {r:g} 0 1 0 {2*r:g} 0a{r:g} {r:g} 0 1 0 {-2*r:g} 0Z')
        elif tag == 'rect':
            x, y, w, h = (float(attr[k]) for k in ('x', 'y', 'width', 'height'))
            rx = float(attr.get('rx', attr.get('ry', 0)))
            ry = float(attr.get('ry', rx))
            rx, ry = min(rx, w / 2), min(ry, h / 2)
            if rx and ry:
                data.append(f'M{x+rx:g} {y:g}h{w-2*rx:g}a{rx:g} {ry:g} 0 0 1 {rx:g} {ry:g}v{h-2*ry:g}a{rx:g} {ry:g} 0 0 1 {-rx:g} {ry:g}h{2*rx-w:g}a{rx:g} {ry:g} 0 0 1 {-rx:g} {-ry:g}v{2*ry-h:g}a{rx:g} {ry:g} 0 0 1 {rx:g} {-ry:g}Z')
            else:
                data.append(f'M{x:g} {y:g}h{w:g}v{h:g}h{-w:g}Z')
        else:
            raise ValueError(f'{source}: {tag}')
    source.with_suffix('.path').write_text(' '.join(data) + '\n')
