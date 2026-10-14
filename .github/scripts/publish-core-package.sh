#!/usr/bin/env bash
# Publish one built core package (DIR/NAME.tar.gz, .sha256, .json) to this commit's canary
# release. The ONE implementation, run by each build leg and by publish-check's republish.
set -euo pipefail
dir="$1"; name="$2"
tag="canary-${GITHUB_SHA:0:12}"
# Every leg races to create the release; the loser finds it and uploads into it.
gh release view "$tag" -R "$GITHUB_REPOSITORY" >/dev/null 2>&1 \
  || gh release create "$tag" -R "$GITHUB_REPOSITORY" --prerelease --target "$GITHUB_SHA" \
       --title "canary ${GITHUB_SHA:0:12}" \
       --notes "Canary core binaries for ${GITHUB_SHA}. No LTO; for the development grid, not for users. Built by .github/workflows/core-binaries.yml." \
  || gh release view "$tag" -R "$GITHUB_REPOSITORY" >/dev/null
# The manifest is what a consumer reads as "published", so it goes up LAST, in its own call,
# only after the archive it names is complete. One call uploading all three let the small
# .json land minutes before the large tarball: the 5090 read the 8fe8e5fd4 Windows manifest at
# 07:09:54Z and its fetch 404'd at 07:10:02Z (BigMama, 2026-10-07). For the tip's own core a
# refused fetch reads as "compile here": an hour of local build over seconds of upload order.
# A rerun replaces an archive a previous run already published: withdraw that run's manifest
# FIRST, or a consumer reads it while the tarball is being re-uploaded and fetches a
# half-replaced archive (Cormac on #4855). Absent on a first run.
gh release delete-asset "$tag" "$name.json" -R "$GITHUB_REPOSITORY" --yes 2>/dev/null || true
gh release upload "$tag" -R "$GITHUB_REPOSITORY" --clobber "$dir/$name.tar.gz" "$dir/$name.tar.gz.sha256"
gh release upload "$tag" -R "$GITHUB_REPOSITORY" --clobber "$dir/$name.json"
