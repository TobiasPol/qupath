#!/usr/bin/env bash
# Fails if a Rust file exceeds 300 lines or a function exceeds 50 lines.
set -euo pipefail
cd "$(dirname "$0")/.."
status=0
while read -r n f; do
  [ "$f" = total ] && continue
  if [ "$n" -gt 300 ]; then echo "file too long ($n lines): $f"; status=1; fi
done < <(find src tests examples -name '*.rs' -print0 | xargs -0 wc -l)
python3 -I - <<'PY' || status=1
import re, glob, sys
bad = False
for f in sorted(glob.glob('src/**/*.rs', recursive=True)):
    lines = open(f).read().split('\n'); i = 0
    while i < len(lines):
        m = re.match(r'^(\s*)(pub(\([^)]*\))?\s+)?(const\s+)?(unsafe\s+)?fn\s+(\w+)', lines[i])
        if m and not lines[i].rstrip().endswith(';'):
            ind = m.group(1); j = i + 1
            while j < len(lines) and not (lines[j].startswith(ind + '}') and len(lines[j].rstrip()) == len(ind) + 1):
                j += 1
            if j - i + 1 > 50:
                print(f"function too long ({j - i + 1} lines): {f}:{i + 1} {m.group(6)}"); bad = True
            i = j
        i += 1
sys.exit(1 if bad else 0)
PY
exit $status
