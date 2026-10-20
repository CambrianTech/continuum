#!/bin/bash
# Git Hook Setup Script — installs hooks from tools/scripts/git-*.sh into
# .git/hooks/ as thin delegators that resolve their target via
# `git rev-parse --show-toplevel`. Each delegator is installed only if
# its target script exists; missing targets are skipped silently so this
# script can run idempotently after a partial cleanup.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo "")"
if [[ -z "$REPO_ROOT" ]]; then
  echo "setup-git-hooks: not inside a git checkout — skipping" >&2
  exit 0
fi

HOOKS_DIR="$REPO_ROOT/.git/hooks"
SRC_DIR="$REPO_ROOT/tools/scripts"
mkdir -p "$HOOKS_DIR"

echo "🔗 GIT HOOKS: Setting up repository validation hooks"
echo "=================================================="

INSTALLED=()
SKIPPED=()

install_hook() {
  local hook_name="$1"      # e.g. pre-push
  local target_script="$2"  # e.g. git-prepush.sh
  local description="$3"    # human-readable

  local target_path="$SRC_DIR/$target_script"
  local hook_path="$HOOKS_DIR/$hook_name"

  if [[ ! -f "$target_path" ]]; then
    echo "⏭️  Skipping $hook_name → tools/scripts/$target_script (target script not present)"
    SKIPPED+=("$hook_name")
    return 0
  fi

  echo "📋 Installing $hook_name → tools/scripts/$target_script — $description"
  cat > "$hook_path" <<EOF
#!/bin/bash
# Git $hook_name hook — delegates to tools/scripts/$target_script.
REPO_ROOT="\$(git rev-parse --show-toplevel)"
exec "\$REPO_ROOT/tools/scripts/$target_script" "\$@"
EOF
  chmod +x "$hook_path"
  INSTALLED+=("$hook_name")
}

install_hook pre-push git-prepush.sh "Rust compile + test + optional native-arch docker push"

# Earlier versions of this script also installed pre-commit → git-precommit.sh
# and post-commit → git-postcommit.sh. Both targets guarded the deleted Node
# src/ tree and are gone; a delegator left pointing at them would fail every
# commit. Remove only delegators this script wrote, never a hand-written hook.
for stale in pre-commit:git-precommit.sh post-commit:git-postcommit.sh; do
  stale_hook="$HOOKS_DIR/${stale%%:*}"
  if [[ -f "$stale_hook" ]] && grep -q "delegates to tools/scripts/${stale#*:}" "$stale_hook"; then
    rm -f "$stale_hook"
    echo "🧹 Removed stale ${stale%%:*} delegator (tools/scripts/${stale#*:} no longer exists)"
  fi
done

echo ""
echo "✅ Git hooks setup complete"
echo "=================================================="
if [[ ${#INSTALLED[@]} -gt 0 ]]; then
  echo "📁 Installed: ${INSTALLED[*]}"
fi
if [[ ${#SKIPPED[@]} -gt 0 ]]; then
  echo "⏭️  Skipped (target script missing): ${SKIPPED[*]}"
fi
echo ""
echo "🛠️ Re-run with: npm run setup:git-hooks"
