#!/bin/bash
# Git Hook Setup Script - Makes hidden .git/hooks/ visible and manageable

echo "🔗 GIT HOOKS: Setting up repository validation hooks"
echo "=================================================="

# Ensure hooks directory exists
mkdir -p .git/hooks

# Setup pre-commit hook
echo "📋 Installing pre-commit hook → scripts/git-precommit.sh"
cat > .git/hooks/pre-commit << 'EOF'
#!/bin/bash
# Git pre-commit hook - Delegates to main script
exec ./scripts/git-precommit.sh
EOF
chmod +x .git/hooks/pre-commit

# Setup post-commit hook
echo "📋 Installing post-commit hook → scripts/git-postcommit.sh"
cat > .git/hooks/post-commit << 'EOF'
#!/bin/bash
# Git post-commit hook - Clean up validation artifacts after successful commits
exec ./scripts/git-postcommit.sh
EOF
chmod +x .git/hooks/post-commit

# Setup pre-push hook
echo "📋 Installing pre-push hook → scripts/git-prepush.sh"
cat > .git/hooks/pre-push << 'EOF'
#!/bin/bash
# Git pre-push hook - Delegates to main script
exec ./scripts/git-prepush.sh
EOF
chmod +x .git/hooks/pre-push

echo ""
echo "✅ Git hooks installed successfully!"
echo "=================================================="
echo "📁 Hook scripts (visible and editable):"
echo "   • scripts/git-precommit.sh   - Comprehensive CRUD + State validation"
echo "   • scripts/git-postcommit.sh  - Cleanup after successful commit"
echo "   • scripts/git-prepush.sh     - Lightweight pre-push checks"
echo ""
echo "🔗 Git integration (hidden but managed):"
echo "   • .git/hooks/pre-commit   → scripts/git-precommit.sh"
echo "   • .git/hooks/post-commit  → scripts/git-postcommit.sh"
echo "   • .git/hooks/pre-push     → scripts/git-prepush.sh"
echo ""
echo "🛠️ Management commands:"
echo "   npm run hooks:setup     - Run this script"
echo "   npm run hooks:test      - Test all hooks"
echo "   npm run hooks:status    - Show hook status"
echo "   npm run hooks:remove    - Remove all hooks"