#!/bin/bash
# Install Continuum git hooks for all developers
# Run this after cloning the repo: bash scripts/install-git-hooks.sh

echo "🔧 Installing Continuum git hooks..."

# Get repository root
REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT"

# Install pre-commit hook
echo "📋 Installing pre-commit hook..."
cp scripts/git-hooks/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit

echo "✅ Git hooks installed successfully!"
echo ""
echo "🎯 The pre-commit hook will now:"
echo "   - Block commits with lingering verification files"
echo "   - Block commits if tests fail" 
echo "   - Block commits if core commands fail"
echo "   - Force verification before allowing commits"
echo ""
echo "💡 This prevents broken commits and ensures system health"