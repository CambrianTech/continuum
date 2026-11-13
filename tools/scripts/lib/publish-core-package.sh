#!/usr/bin/env bash
# One owner for putting a built core package on this commit's canary release.
#   publish DIR NAME   upload DIR/NAME.{tar.gz,tar.gz.sha256,json}; run by each build leg
#   check   DIR        for every DIR/*.json, republish any leg whose release manifest is not
#                      byte-identical to its artifact; run by publish-check after every leg
# Needs GITHUB_SHA and GITHUB_REPOSITORY (the Actions environment) and an authenticated gh.
set -euo pipefail
tag="canary-${GITHUB_SHA:0:12}"
repo="$GITHUB_REPOSITORY"

assets() { gh release view "$tag" -R "$repo" --json assets -q '.assets[].name'; }

ensure_release() {
  # Every leg races to create the release; the loser finds it and uploads into it.
  gh release view "$tag" -R "$repo" >/dev/null 2>&1 \
    || gh release create "$tag" -R "$repo" --prerelease --target "$GITHUB_SHA" \
         --title "canary ${GITHUB_SHA:0:12}" \
         --notes "Canary core binaries for ${GITHUB_SHA}. No LTO; for the development grid, not for users. Built by .github/workflows/core-binaries.yml." \
    || gh release view "$tag" -R "$repo" >/dev/null
}

publish() {
  local dir="$1" name="$2" listed
  ensure_release
  # The manifest is what a consumer reads as "published", so it goes up LAST, in its own call,
  # only after the archive it names is complete: one call uploading all three let the small
  # .json land minutes before the large tarball, and the 5090 read the 8fe8e5fd4 Windows
  # manifest at 07:09:54Z and 404'd on its fetch at 07:10:02Z (BigMama, 2026-10-07).
  # A rerun replaces an archive a previous run published: withdraw that run's manifest FIRST,
  # or a consumer reads it while the tarball is re-uploaded (Cormac on #4855). The withdrawal
  # FAILS CLOSED: an unreadable asset list or a failed delete stops here, never leaves an old
  # manifest standing over a half-replaced archive (Codex helper on #5043).
  listed="$(assets)"
  if grep -qx "$name.json" <<<"$listed"; then
    gh release delete-asset "$tag" "$name.json" -R "$repo" --yes
  fi
  gh release upload "$tag" -R "$repo" --clobber "$dir/$name.tar.gz" "$dir/$name.tar.gz.sha256"
  gh release upload "$tag" -R "$repo" --clobber "$dir/$name.json"
}

# Published = the release carries THIS build's manifest, byte for byte. A manifest left by an
# earlier run of the same commit names an older archive, so its name alone proves nothing.
published_exactly() {
  local dir="$1" name="$2" got
  got="$(mktemp -d)"
  if gh release download "$tag" -R "$repo" -p "$name.json" -D "$got" --clobber >/dev/null 2>&1 \
       && cmp -s "$got/$name.json" "$dir/$name.json"; then
    rm -rf "$got"; return 0
  fi
  rm -rf "$got"; return 1
}

check() {
  local dir="$1" manifest name missing=()
  shopt -s nullglob
  local manifests=("$dir"/*.json)
  if [ "${#manifests[@]}" -eq 0 ]; then
    echo "::error::no leg produced a package" >&2
    return 1
  fi
  for manifest in "${manifests[@]}"; do
    name="$(basename "$manifest" .json)"
    if published_exactly "$dir" "$name"; then
      echo "$name: published"
      continue
    fi
    echo "$name: not on $tag as built; republishing from its artifact"
    # one leg's failure never stops the next (Codex root on #5043). A separate process, not a
    # subshell: `set -e` is suppressed inside an `if` condition, which would let a failed
    # withdrawal carry on to the uploads.
    if ! bash "${BASH_SOURCE[0]}" publish "$dir" "$name"; then
      missing+=("$name")
    fi
  done
  if [ "${#missing[@]}" -gt 0 ]; then
    echo "::error::still unpublished: ${missing[*]} (rerun this job: gh run rerun --failed)" >&2
    return 1
  fi
}

case "${1:-}" in
  publish) publish "$2" "$3" ;;
  check) check "$2" ;;
  *) echo "usage: $0 publish DIR NAME | check DIR" >&2; exit 2 ;;
esac
