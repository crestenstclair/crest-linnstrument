#!/usr/bin/env python3
"""Embed checked-in manual media and firmware reference data for offline reading."""
from pathlib import Path
import base64
import json
import re

ROOT = Path(__file__).resolve().parents[1]
MEDIA = ROOT / 'docs/media'


def embed(match):
    mime, name = match.groups()
    path = (MEDIA / name).resolve()
    if path.parent != MEDIA.resolve():
        raise ValueError('Manual assets must be direct children of docs/media')
    return 'data:' + mime + ';base64,' + base64.b64encode(path.read_bytes()).decode()


def main():
    source = (ROOT / 'docs/manual-source.html').read_text()
    reference = json.loads((MEDIA / 'surface-reference.json').read_text())
    source = source.replace('{{surface-reference}}', json.dumps(reference, ensure_ascii=False))
    source = re.sub(r'\{\{asset:([^:]+):([^}]+)\}\}', embed, source)
    if '{{asset:' in source or '{{surface-reference}}' in source:
        raise ValueError('Unexpanded manual placeholder')
    target = ROOT / 'docs/manual.html'
    target.write_text(source)
    print(f'{target}: {target.stat().st_size} bytes')


if __name__ == '__main__':
    main()
