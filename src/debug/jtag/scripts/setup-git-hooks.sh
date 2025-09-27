#!/bin/bash
set -e

echo "🔧 Setting up Git hooks for bulletproof precommit validation"
echo "==========================================================="

# Navigate to project root
cd "$(dirname "$0")/.."

# Method 1: Direct git hook (fallback if no husky)
echo "📋 Option 1: Direct git hook setup"
mkdir -p .git/hooks

cat > .git/hooks/pre-commit << 'EOF'
#!/bin/sh
# Git pre-commit hook - Bulletproof validation with proof artifacts
cd "$(dirname "$0")/../.."
exec ./scripts/git-precommit.sh
EOF

chmod +x .git/hooks/pre-commit
echo "✅ Direct git hook installed at .git/hooks/pre-commit"

# Method 2: Husky integration (if husky is available)
if command -v npx >/dev/null 2>&1 && npm list husky >/dev/null 2>&1; then
    echo ""
    echo "📋 Option 2: Husky integration detected"

    # Initialize husky if not already done
    if [ ! -d ".husky" ]; then
        npx husky init
        echo "✅ Husky initialized"
    fi

    # Create husky pre-commit hook
    npx husky add .husky/pre-commit "./scripts/git-precommit.sh"
    echo "✅ Husky pre-commit hook configured"
else
    echo ""
    echo "📋 Option 2: Husky not detected - using direct git hook only"
fi

# Method 3: npm script integration
echo ""
echo "📋 Option 3: npm script integration"
echo "   Run manually: npm run test:precommit"
echo "   Run in CI/CD: npm run test:precommit"

echo ""
echo "🎉 Git hooks setup complete!"
echo "================================"
echo "✅ Pre-commit validation will now run automatically"
echo "✅ Commits will be blocked if any validation fails"
echo "✅ Successful sessions will be archived as proof"
echo ""
echo "🔧 Test the setup:"
echo "   ./scripts/git-precommit.sh"
echo "   # or"
echo "   npm run test:precommit"