#!/usr/bin/env bash
# What this catches (Codex root and helper on #5043): a transient GitHub API error in one build
# leg's publish threw away a good ~1 h build (run 38588068431). publish-check must:
#   - republish a leg whose manifest is missing, from its saved package, with no build;
#   - not let one leg's failed republish stop the next leg;
#   - fail when a republish fails;
#   - skip a leg already published byte-for-byte, but republish a STALE manifest an earlier run
#     left;
# and publish must fail CLOSED when withdrawing the old manifest fails.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
publish="$script_dir/../lib/publish-core-package.sh"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/continuum-publish.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

export GITHUB_SHA=0123456789abcdef0123456789abcdef01234567 GITHUB_REPOSITORY=Example/repo
export RELEASE="$scratch/release" TRACE="$scratch/trace" FAILS="$scratch/fails"
mkdir -p "$scratch/bin"
# A fake gh: the release's assets are files in $RELEASE; "$FAILS/<op>:<asset>" makes that op fail.
cat > "$scratch/bin/gh" <<'GH'
#!/usr/bin/env bash
echo "gh $*" >> "$TRACE"
fails() { [ -f "$FAILS/$1" ]; }
[ "$1" = release ] || exit 2
op="$2"; shift 2
case "$op" in
  view)
    [ -d "$RELEASE" ] || exit 1
    if [[ " $* " == *" --json assets "* ]]; then
      fails list && { echo "error connecting to api.github.com" >&2; exit 1; }
      ls "$RELEASE"
    fi ;;
  create) mkdir -p "$RELEASE" ;;
  delete-asset)
    name="$2"
    fails "delete:$name" && { echo "error connecting to api.github.com" >&2; exit 1; }
    rm -f "$RELEASE/$name" ;;
  upload)
    for f in "$@"; do
      case "$f" in -R|--clobber|Example/repo|canary-*) continue ;; esac
      fails "upload:$(basename "$f")" && { echo "error connecting to api.github.com" >&2; exit 1; }
      cp "$f" "$RELEASE/"
    done ;;
  download)
    pattern="" dest=""
    while [ $# -gt 0 ]; do case "$1" in -p) pattern="$2"; shift ;; -D) dest="$2"; shift ;; esac; shift; done
    [ -f "$RELEASE/$pattern" ] || exit 1
    cp "$RELEASE/$pattern" "$dest/" ;;
  *) exit 2 ;;
esac
GH
chmod +x "$scratch/bin/gh"
export PATH="$scratch/bin:$PATH"

package() {  # package DIR NAME BUILD-ID: a built leg's three files
  mkdir -p "$1"
  echo "archive of $2 build $3" > "$1/$2.tar.gz"
  echo "sha of $2 build $3" > "$1/$2.tar.gz.sha256"
  echo "{\"name\": \"$2\", \"build\": \"$3\"}" > "$1/$2.json"
}
reset() { rm -rf "$RELEASE" "$FAILS" "$scratch/dist"; mkdir -p "$FAILS"; : > "$TRACE"; }
uploads_of() { grep -c "^gh release upload .*$1" "$TRACE" || true; }

# A: the first leg's in-leg publish failed (nothing on the release); the second published.
# check republishes ONLY the first, from its saved package, and skips the second.
reset; package "$scratch/dist" leg-a 1; package "$scratch/dist" leg-b 1
bash "$publish" publish "$scratch/dist" leg-b >/dev/null
: > "$TRACE"
bash "$publish" check "$scratch/dist" > "$scratch/out" 2>&1 || fail "A: check failed: $(cat "$scratch/out")"
cmp -s "$RELEASE/leg-a.json" "$scratch/dist/leg-a.json" || fail "A: leg-a was not republished from its artifact"
[ "$(uploads_of leg-b)" = 0 ] || fail "A: an exactly-published leg was re-uploaded"
grep -q "leg-b: published" "$scratch/out" || fail "A: leg-b not reported published"

# B: the retry itself fails: check fails and names the leg, yet still checks the next leg.
reset; package "$scratch/dist" leg-a 1; package "$scratch/dist" leg-b 1
touch "$FAILS/upload:leg-a.tar.gz"
if bash "$publish" check "$scratch/dist" > "$scratch/out" 2>&1; then fail "B: check passed with leg-a unpublished"; fi
grep -q "still unpublished: leg-a" "$scratch/out" || fail "B: the failed leg was not named: $(cat "$scratch/out")"
cmp -s "$RELEASE/leg-b.json" "$scratch/dist/leg-b.json" || fail "B: leg-a's failure stopped leg-b"
[ ! -f "$RELEASE/leg-a.json" ] || fail "B: a manifest went up without its archive"

# C: a STALE manifest from an earlier run of this commit is not "published": republished.
reset; package "$scratch/old" leg-a 0; bash "$publish" publish "$scratch/old" leg-a >/dev/null
package "$scratch/dist" leg-a 1
bash "$publish" check "$scratch/dist" > "$scratch/out" 2>&1 || fail "C: check failed: $(cat "$scratch/out")"
cmp -s "$RELEASE/leg-a.json" "$scratch/dist/leg-a.json" || fail "C: a stale manifest was trusted by name"

# D: withdrawing the old manifest fails: publish fails CLOSED, uploading nothing over it.
reset; package "$scratch/old" leg-a 0; bash "$publish" publish "$scratch/old" leg-a >/dev/null
package "$scratch/dist" leg-a 1; touch "$FAILS/delete:leg-a.json"; : > "$TRACE"
if bash "$publish" publish "$scratch/dist" leg-a > "$scratch/out" 2>&1; then fail "D: publish passed over a failed withdrawal"; fi
[ "$(uploads_of leg-a)" = 0 ] || fail "D: uploads went ahead after a failed withdrawal"
# and an unreadable asset list fails closed too
rm -f "$FAILS/delete:leg-a.json"; touch "$FAILS/list"; : > "$TRACE"
if bash "$publish" publish "$scratch/dist" leg-a > "$scratch/out" 2>&1; then fail "D: publish passed with an unreadable asset list"; fi
[ "$(uploads_of leg-a)" = 0 ] || fail "D: uploads went ahead without knowing the asset list"

echo "✓ publish-check republishes only missing or stale legs from their saved packages, continues past a failed leg and fails naming it; publish fails closed on a failed withdrawal"
