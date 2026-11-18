"""Sort generated notice records and their crate lists while preserving license contents."""
from pathlib import Path
import sys

SEPARATOR = '\n' + '=' * 80 + '\n'


def normalize(text):
    text = '\n'.join(line.rstrip() for line in text.replace('\r\n', '\n').split('\n'))
    header, *records = text.split(SEPARATOR)
    canonical = []
    for record in records:
        before, marker, after = record.partition('\nUsed by:\n')
        if marker:
            lines = after.splitlines(keepends=True)
            count = 0
            while count < len(lines) and lines[count].startswith('  - '):
                count += 1
            record = before + marker + ''.join(sorted(lines[:count])) + ''.join(lines[count:])
        canonical.append(record)
    return SEPARATOR.join([header, *sorted(canonical)])


if __name__ == '__main__':
    sys.stdout.write(normalize(Path(sys.argv[1]).read_text()))
