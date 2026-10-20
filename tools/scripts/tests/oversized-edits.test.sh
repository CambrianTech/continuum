#!/usr/bin/env bash
# what this catches: the two ways past the oversized-edit gate Codex found on #5221: a rename plus
# edit (numstat's "old => new" is no path) and a final line with no newline (wc -l misses it).
set -euo pipefail
gate="$(cd "$(dirname "$0")/.." && pwd)/oversized-edits.sh"
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT; cd "$t"
git init -q && git config user.email t@t && git config user.name t
awk 'BEGIN{for(i=0;i<1200;i++) print "// x"}' > big.rs; awk 'BEGIN{for(i=0;i<10;i++) print "// s"}' > small.rs
git add . && git commit -qm base && git branch -q base
expect() { if "$gate" base >/dev/null; then got=0; else got=1; fi; [ "$got" = "$1" ] || { echo "FAIL: $2"; exit 1; }; }
echo "// edit" >> small.rs && git commit -qam small; expect 0 "a small file's edit passes"
git mv big.rs moved.rs && echo "// edit" >> moved.rs && git commit -qam rename; expect 1 "a renamed and edited monolith fails at its destination"
git reset -q --hard base
awk 'BEGIN{for(i=0;i<1000;i++) print "x"}' > nonl.rs && printf 'y' >> nonl.rs && git add nonl.rs && git commit -qm nonl; expect 1 "1,001 lines with no final newline fails"
echo "oversized-edits gate: 3/3"
