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
#   scripts/verify-image-revisions.sh
#
# Auth: uses `docker buildx imagetools` which reuses the existing
# `docker login ghcr.io` state. No PAT handling in the script — if
# imagetools can't reach the registry, the underlying `docker login`
# isn't valid. Previously this script did raw `curl -H "Authorization:
# Bearer $TOKEN" https://ghcr.io/v2/.../blobs/<digest>` which 404'd in
# practice: the script was passing the per-arch MANIFEST digest to the
# /blobs/ endpoint (manifests live under /manifests/, not /blobs/), so
# the auth-scoped pull token was being asked to fetch a blob that
# doesn't exist under that digest. On top of that, ghcr's pull token
# from `/token?scope=repository:x:pull` can refuse blob fetches when
# the caller is gh's default oauth scope vs a PAT with read:packages.
# Both failure modes disappear when we let docker's credential helper
# handle auth.
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

REGISTRY_HOST="ghcr.io"
DEFAULT_IMAGES="ghcr.io/cambriantech/continuum-core-vulkan:ghcr.io/cambriantech/continuum-core-cuda:ghcr.io/cambriantech/continuum-livekit-bridge:ghcr.io/cambriantech/continuum-node:ghcr.io/cambriantech/continuum-model-init:ghcr.io/cambriantech/continuum-widgets"
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

# image_relevant_paths — given a full image ref, return the
# space-separated git path globs that affect this image's docker bits.
# Used by the smart staleness check below: if a stale revision label
# differs from HEAD but the diff between them touches NONE of these
# paths, the image bits would be identical — skip the rebuild.
#
# Conservative by design: when in doubt, include the path. A false
# positive (we list a path that doesn't actually affect the image)
# costs us a wasted rebuild we'd have done anyway under the old
# behavior. A false negative (we miss a path that DOES affect the
# image) silently ships stale bits — much worse. Add paths
# generously, prune only when proven unused.
image_relevant_paths() {
  local ref="$1"
  case "$ref" in
    *continuum-core-cuda*|*continuum-core-vulkan*|*continuum-core*|*continuum-livekit-bridge*)
      echo "core docker/continuum-core.Dockerfile docker/continuum-core-cuda.Dockerfile docker/continuum-core-vulkan.Dockerfile docker/livekit-bridge.Dockerfile docker/livekit-entrypoint.sh docker/livekit.yaml"
      ;;
    *continuum-node*)
      # node-server bakes most of src/ + node_modules/ via npm ci. Anything
      # under src/ that isn't workers/* affects this image. Cargo files
      # included because the Dockerfile reads workers/*/Cargo.* metadata.
      echo "src docker/node-server.Dockerfile"
      ;;
    *continuum-widgets*)
      echo "src/widgets src/browser src/shared docker/widget-server.Dockerfile"
      ;;
    *continuum-model-init*)
      echo "tools/scripts/install-livekit.sh tools/scripts/download-voice-models.sh docker/model-init.Dockerfile"
      ;;
    *)
      # Unknown image — be safe, treat any change as relevant.
      echo "."
      ;;
  esac
}

# can_diff_locally — return 0 if both SHAs are present in the local git
# repo and a `git diff` between them will succeed. CI runners typically
# checkout fetch-depth=1 so older SHAs may be missing; fall back to
# treat-as-stale when we can't introspect the diff.
can_diff_locally() {
  local a="$1"
  local b="$2"
  git cat-file -e "$a^{commit}" 2>/dev/null && git cat-file -e "$b^{commit}" 2>/dev/null
}

# fetch_revision_label — given a repo (without tag) and the per-arch
# manifest digest, walk index → manifest → config blob → labels and
# extract `org.opencontainers.image.revision`. Returns empty if any
# hop fails or the label is absent.
fetch_revision_label() {
  local repo="$1"        # e.g. ghcr.io/cambriantech/continuum-core
  local manifest_digest="$2"

  local manifest
  manifest=$(docker buildx imagetools inspect --raw "${repo}@${manifest_digest}" 2>/dev/null)
  [[ -z "$manifest" ]] && return

  local config_digest
  config_digest=$(echo "$manifest" | jq -r '.config.digest // empty' 2>/dev/null)
  [[ -z "$config_digest" || "$config_digest" == "null" ]] && return

  local config
  config=$(docker buildx imagetools inspect --raw "${repo}@${config_digest}" 2>/dev/null)
  [[ -z "$config" ]] && return

  echo "$config" | jq -r '.config.Labels["org.opencontainers.image.revision"] // empty' 2>/dev/null
}

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

  RAW=$(docker buildx imagetools inspect --raw "$REF" 2>/dev/null || echo '{}')

  # For multi-arch indexes: enumerate per-platform manifests. Skip the
  # `unknown/unknown` attestation manifests buildx adds alongside real
  # arch manifests — those are sbom/provenance, not image configs with
  # revision labels. For single-arch images (no manifests array), use
  # the top-level config digest directly so the script still works on
  # Dockerfiles that emit single-platform artifacts.
  ARCH_LIST=$(echo "$RAW" | jq -r '
    if (.manifests // [] | length) > 0 then
      [.manifests[]
       | select(.platform.os == "linux")
       | select(.platform.architecture != "unknown")
       | "\(.platform.architecture):\(.digest)"] | .[]
    else
      "amd64:\(.config.digest // empty)"
    end
  ' 2>/dev/null)

  if [[ -z "$ARCH_LIST" ]]; then
    echo "  ⚠️  No manifest entries — image may not exist yet at this tag"
    continue
  fi

  # Track whether we saw amd64 for this image. A multi-arch tag that is
  # missing the amd64 entry entirely is a hard failure — the user-facing
  # target cannot ship without its primary arch.
  SAW_AMD64=0

  for entry in $ARCH_LIST; do
    ARCH="${entry%%:*}"
    MANIFEST_DIGEST="${entry#*:}"
    [[ -z "$MANIFEST_DIGEST" || "$MANIFEST_DIGEST" == "null" ]] && continue
    [[ "$ARCH" == "amd64" ]] && SAW_AMD64=1

    # For single-arch-as-top-level (jq fallback branch above), the
    # digest is already the config digest — no intermediate manifest
    # hop needed. Detect by trying the two-hop path first and falling
    # back to a direct config fetch. Most real images hit the two-hop
    # path since buildx produces OCI indexes even for single-platform
    # pushes.
    REV=$(fetch_revision_label "$IMAGE" "$MANIFEST_DIGEST")

    # Fallback: maybe the extracted digest IS a config blob (rare,
    # happens when `inspect --raw` returns an image manifest directly
    # rather than an index). One hop.
    if [[ -z "$REV" ]]; then
      CONFIG_DIRECT=$(docker buildx imagetools inspect --raw "${IMAGE}@${MANIFEST_DIGEST}" 2>/dev/null)
      REV=$(echo "$CONFIG_DIRECT" | jq -r '.config.Labels["org.opencontainers.image.revision"] // empty' 2>/dev/null)
    fi

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
      # Smart staleness check: a label-vs-HEAD SHA mismatch isn't a real
      # stale unless the diff between them touches files that affect this
      # image's docker bits. Workflow YAML / docs / non-context changes
      # produce IDENTICAL image layers across SHAs — rebuilding for a
      # label update is pure waste (we hit this 2026-04-24, ~30min GHA
      # for byte-identical bits). Skip the rebuild when the diff doesn't
      # touch this image's relevant paths.
      RELEVANT_PATHS=$(image_relevant_paths "$IMAGE")
      if can_diff_locally "$REV" "$EXPECTED_SHA"; then
        if [[ -n "$RELEVANT_PATHS" ]] \
           && ! git diff --name-only "$REV" "$EXPECTED_SHA" -- $RELEVANT_PATHS 2>/dev/null | grep -q .; then
          echo "  ✅ $ARCH: revision $REV ≠ HEAD $EXPECTED_SHA but no image-relevant diff — bits match, skipping rebuild"
          continue
        fi
      fi
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

  # Missing-amd64-entry detection: if the tag is multi-arch but has no
  # amd64 platform at all, that's the tag-overwrite race (arm64 push
  # clobbered the multi-arch manifest). This is a hard fail separate
  # from "revision label absent."
  if [[ "$SAW_AMD64" -eq 0 ]]; then
    # Only flag if the index actually has multiple arch entries — a
    # single-arch-only image shouldn't trip this.
    ARCH_COUNT=$(echo "$ARCH_LIST" | wc -l | tr -d ' ')
    if [[ "$ARCH_COUNT" -gt 0 ]]; then
      echo "  ❌ amd64: MISSING from multi-arch manifest — tag-overwrite race (arm64 push clobbered amd64)"
      echo "$REF" >> "$STALE_AMD64_OUT"
      FAILED=1
    fi
  fi
done

if [ "$WARN_ARM64" -ne 0 ]; then
  echo ""
  echo "⚠️  arm64 stale on $(wc -l < "$STALE_ARM64_OUT" | tr -d ' ') image(s):"
  while IFS= read -r REF; do echo "     - $REF"; done < "$STALE_ARM64_OUT"
  echo "   Mac M-series dev: run \`scripts/push-current-arch.sh\` to refresh."
  echo "   Not blocking today, but CI will not rebuild this automatically."
fi

if [ "$FAILED" -ne 0 ]; then
  echo ""
  echo "❌ STALE-IMAGE GATE FAILED — amd64 image(s) at :$TAG built from a different commit."
  echo "   The user-facing target must always be current."
  echo ""
  echo "   Fix:"
  echo "     Linux/amd64 host: run \`scripts/push-current-arch.sh\`"
  echo "     Then re-run this workflow."
  echo ""
  echo "   CI is a check here, not a builder; it will not auto-rebuild stale Rust images."
  exit 1
fi
echo ""
echo "✅ amd64 images at tag $TAG built from HEAD SHA $EXPECTED_SHA"
exit 0
