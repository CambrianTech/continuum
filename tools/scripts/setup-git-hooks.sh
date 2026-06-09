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
  local hook_name="$1"      # e.g. pre-commit
  local target_script="$2"  # e.g. git-precommit.sh
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

install_hook pre-commit  git-precommit.sh  "Comprehensive CRUD + state validation"
install_hook post-commit git-postcommit.sh "Post-commit cleanup"
install_hook pre-push    git-prepush.sh    "Compile + test + native-arch docker push"

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
echo "🛠️ Management commands:"
echo "   npm run hooks:setup     - Run this script"
echo "   npm run hooks:test      - Test all hooks"
echo "   npm run hooks:status    - Show hook status"
echo "   npm run hooks:remove    - Remove all hooks"
