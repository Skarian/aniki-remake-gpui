"""Compare original capture files before diagnosing a visual regression.

Identical encoded bytes prove identical images. Different encoded bytes do NOT
prove different pixels (metadata, compression and cursor position can differ).
This checks stored evidence only; it does not automate or capture the desktop.
"""
import argparse
import hashlib
import json
from pathlib import Path


def describe(path):
    data = Path(path).read_bytes()
    fmt = ('png' if data.startswith(b'\x89PNG\r\n\x1a\n') else
           'jpeg' if data.startswith(b'\xff\xd8\xff') else 'unknown')
    return data, {
        'path': str(path), 'format': fmt, 'bytes': len(data),
        'sha256': hashlib.sha256(data).hexdigest(),
    }


def compare(first, second):
    a, a_info = describe(first)
    b, b_info = describe(second)
    return {
        'first': a_info, 'second': b_info,
        'encoded_files_identical': a == b,
        'conclusion': ('Identical image evidence; these files cannot demonstrate disappearing labels.'
                       if a == b else 'Files differ; inspect original pixels before diagnosing a rendering bug.'),
    }


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('first')
    parser.add_argument('second')
    args = parser.parse_args()
    print(json.dumps(compare(args.first, args.second), indent=2))
