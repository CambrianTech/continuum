#!/usr/bin/env bash
# verify-image-revisions.sh — assert each pushed image's
# `org.opencontainers.image.revision` label matches an expected SHA,
# per-arch with separate hard/warn policies.
#
# This script is the single source of truth for the SHA-revision gate.
# Both `verify-architectures` (initial) and `verify-after-rebuild`
# (post-CI-rebuild) invoke this same script. A developer can also run
# it manually to check whether the registry is current before merge.
#
# Per Joel: "you can't have one [check] that's yaml and another that's
# shell. you have to reuse otherwise they diverge." (2026-04-23)
#
# Usage:
#   EXPECTED_SHA=<full sha> TAG=<image tag> \
#   GHCR_USER=<user> GHCR_TOKEN=<token> \
#   scripts/verify-image-revisions.sh
#
# Optional env:
#   STALE_ARM64_OUT=<path>  Write newline-separated list of stale arm64
#                           image refs to this file (for CI matrix input).
#   STALE_AMD64_OUT=<path>  Same for amd64.
#   IMAGES=<colon-list>     Override the image list (default = all 7).
#
# Exit codes:
#   0 = no amd64 stale (arm64 stale OK — warning-only until #965 lands)
#   1 = amd64 stale on at least one image
#   2 = usage / pre-flight error

set -uo pipefail

if [[ -z "${EXPECTED_SHA:-}" ]]; then
  echo "ERROR: EXPECTED_SHA env var required" >&2
  exit 2
fi
if [[ -z "${TAG:-}" ]]; then
  echo "ERROR: TAG env var required" >&2
  exit 2
fi
if [[ -z "${GHCR_USER:-}" || -z "${GHCR_TOKEN:-}" ]]; then
  echo "ERROR: GHCR_USER and GHCR_TOKEN env vars required for blob fetch" >&2
  exit 2
fi

REGISTRY_HOST="ghcr.io"
DEFAULT_IMAGES="ghcr.io/cambriantech/continuum-core:ghcr.io/cambriantech/continuum-core-vulkan:ghcr.io/cambriantech/continuum-core-cuda:ghcr.io/cambriantech/continuum-livekit-bridge:ghcr.io/cambriantech/continuum-node:ghcr.io/cambriantech/continuum-model-init:ghcr.io/cambriantech/continuum-widgets"
IMAGES="${IMAGES:-$DEFAULT_IMAGES}"

STALE_ARM64_OUT="${STALE_ARM64_OUT:-/dev/null}"
STALE_AMD64_OUT="${STALE_AMD64_OUT:-/dev/null}"
: > "$STALE_ARM64_OUT"
: > "$STALE_AMD64_OUT"

echo "Expected revision: $EXPECTED_SHA"
echo "Tag:               $TAG"
echo "Policy: amd64 = HARD, arm64 = WARN (until #965 lands CI auto-rebuild)"
echo ""

FAILED=0
WARN_ARM64=0

# Iterate the colon-separated image list. Bash IFS swap so the `for`
# splits on `:` without regex / xargs.
SAVED_IFS="$IFS"
IFS=':'
# shellcheck disable=SC2206
IMAGE_ARRAY=($IMAGES)
IFS="$SAVED_IFS"

for IMAGE in "${IMAGE_ARRAY[@]}"; do
  REF="$IMAGE:$TAG"
  echo "━━━ $REF ━━━"
  REPO_PATH="${IMAGE#"$REGISTRY_HOST/"}"

  # One token per image; reused for amd64 + arm64 blob fetches.
  TOKEN=$(curl -fsSL -u "$GHCR_USER:$GHCR_TOKEN" \
    "https://$REGISTRY_HOST/token?scope=repository:$REPO_PATH:pull" \
    | jq -r .token 2>/dev/null)

  RAW=$(docker buildx imagetools inspect --raw "$REF" 2>/dev/null || echo '{}')

  # For multi-arch indexes: enumerate per-platform manifests.
  # For single-arch images (no manifests array): treat the top-level
  # config as amd64.
  ARCH_LIST=$(echo "$RAW" | jq -r '
    if (.manifests // [] | length) > 0 then
      [.manifests[] | select(.platform.os == "linux") | "\(.platform.architecture):\(.digest)"] | .[]
    else
      "amd64:\(.config.digest // empty)"
    end
  ' 2>/dev/null)

  if [[ -z "$ARCH_LIST" ]]; then
    echo "  ⚠️  No manifest entries — image may not exist yet at this tag"
    continue
  fi

  for entry in $ARCH_LIST; do
    ARCH="${entry%%:*}"
    CONFIG_DIGEST="${entry#*:}"
    [[ -z "$CONFIG_DIGEST" || "$CONFIG_DIGEST" == "null" ]] && continue
    REV=$(curl -fsSL \
      -H "Authorization: Bearer $TOKEN" \
      -H "Accept: application/vnd.oci.image.config.v1+json" \
      "https://$REGISTRY_HOST/v2/$REPO_PATH/blobs/$CONFIG_DIGEST" \
      | jq -r '.config.Labels["org.opencontainers.image.revision"] // empty' 2>/dev/null)
    if [[ -z "$REV" ]]; then
      if [[ "$ARCH" == "amd64" ]]; then
        echo "  ❌ amd64: no org.opencontainers.image.revision label — pre-gate build, refresh required"
        echo "$REF" >> "$STALE_AMD64_OUT"
        FAILED=1
      else
        echo "  ⚠️  $ARCH: no revision label (pre-gate build) — re-push from arm64 host to refresh"
        echo "$REF" >> "$STALE_ARM64_OUT"
        WARN_ARM64=1
      fi
    elif [[ "$REV" != "$EXPECTED_SHA" ]]; then
      if [[ "$ARCH" == "amd64" ]]; then
        echo "  ❌ amd64: STALE (revision $REV ≠ HEAD $EXPECTED_SHA) — Linux dev rebuild required"
        echo "$REF" >> "$STALE_AMD64_OUT"
        FAILED=1
      else
        echo "  ⚠️  $ARCH: STALE (revision $REV ≠ HEAD $EXPECTED_SHA) — Mac dev rebuild required (warning-only until #965)"
        echo "$REF" >> "$STALE_ARM64_OUT"
        WARN_ARM64=1
      fi
    else
      echo "  ✅ $ARCH: revision matches HEAD"
    fi
  done
done

if [ "$WARN_ARM64" -ne 0 ]; then
  echo ""
  echo "⚠️  arm64 stale on $(wc -l < "$STALE_ARM64_OUT" | tr -d ' ') image(s):"
  while IFS= read -r REF; do echo "     - $REF"; done < "$STALE_ARM64_OUT"
  echo "   Mac M-series dev: run \`scripts/push-current-arch.sh\` to refresh."
  echo "   Not blocking — CI auto-rebuild will catch this once #965 lands GitHub arm64 runner support."
fi

if [ "$FAILED" -ne 0 ]; then
  echo ""
  echo "❌ STALE-IMAGE GATE FAILED — amd64 image(s) at :$TAG built from a different commit."
  echo "   The user-facing target must always be current. Re-push from the Linux/amd64 host and re-run."
  exit 1
fi
echo ""
echo "✅ amd64 images at tag $TAG built from HEAD SHA $EXPECTED_SHA"
exit 0
