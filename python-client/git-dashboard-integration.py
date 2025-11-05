#!/usr/bin/env python3
"""
Git Dashboard Integration
========================
Integrates the AI agent dashboard with git workflows and Continuum's test system.
"""

import subprocess
import sys
import json
import re
from pathlib import Path
from datetime import datetime

def run_continuum_tests():
    """Run tests via Continuum's built-in test command"""
    try:
        # Find the correct path to ai-portal.py
        current_dir = Path.cwd()
        portal_path = None
        
        # Check if we're in python-client directory
        if (current_dir / 'ai-portal.py').exists():
            portal_path = str(current_dir / 'ai-portal.py')
        # Check if we're in project root
        elif (current_dir / 'python-client' / 'ai-portal.py').exists():
            portal_path = str(current_dir / 'python-client' / 'ai-portal.py')
        else:
            return False, "", "Could not find ai-portal.py"
        
        # Use ai-portal to run the tests command
        result = subprocess.run([
            'python3', portal_path, 
            '--cmd', 'tests'
        ], capture_output=True, text=True)
        
        return result.returncode == 0, result.stdout, result.stderr
    except Exception as e:
        return False, "", str(e)

def get_dashboard_status():
    """Get current dashboard status"""
    try:
        # Find the correct path to ai-portal.py
        current_dir = Path.cwd()
        portal_path = None
        
        # Check if we're in python-client directory
        if (current_dir / 'ai-portal.py').exists():
            portal_path = str(current_dir / 'ai-portal.py')
        # Check if we're in project root
        elif (current_dir / 'python-client' / 'ai-portal.py').exists():
            portal_path = str(current_dir / 'python-client' / 'ai-portal.py')
        else:
            return None, "Could not find ai-portal.py"
        
        result = subprocess.run([
            'python3', portal_path,
            '--broken'
        ], capture_output=True, text=True)
        
        if result.returncode == 0:
            # Parse broken commands count
            lines = result.stdout.split('\n')
            for line in lines:
                if 'Found' in line and 'broken commands' in line:
                    count_match = re.search(r'Found (\d+) broken commands', line)
                    if count_match:
                        return int(count_match.group(1)), result.stdout
            return 0, result.stdout
        return None, result.stderr
    except Exception as e:
        return None, str(e)

def get_git_status():
    """Get current git status"""
    try:
        # Get staged files
        staged_result = subprocess.run([
            'git', 'diff', '--cached', '--name-only'
        ], capture_output=True, text=True)
        
        # Get current branch
        branch_result = subprocess.run([
            'git', 'rev-parse', '--abbrev-ref', 'HEAD'
        ], capture_output=True, text=True)
        
        return {
            'staged_files': staged_result.stdout.strip().split('\n') if staged_result.stdout.strip() else [],
            'branch': branch_result.stdout.strip()
        }
    except Exception as e:
        return {'staged_files': [], 'branch': 'unknown', 'error': str(e)}

def enhance_commit_message(original_msg, dashboard_info=None, test_results=None):
    """Enhance commit message with dashboard and test information"""
    enhanced = original_msg
    
    if dashboard_info:
        broken_count, dashboard_output = dashboard_info
        enhanced += f"\n\n📊 Dashboard Status: {broken_count} broken commands"
        
        if broken_count == 0:
            enhanced += " 🎉 All commands stable!"
        elif broken_count <= 3:
            enhanced += " ⚠️ Few issues remaining"
        else:
            enhanced += " 🚨 Multiple issues need attention"
    
    if test_results:
        test_passed, test_output, test_error = test_results
        if test_passed:
            enhanced += "\n✅ Tests: PASSED"
        else:
            enhanced += "\n❌ Tests: FAILED"
            if test_error:
                enhanced += f" - {test_error[:100]}..."
    
    enhanced += f"\n\n🤖 Generated with dashboard integration"
    enhanced += f"\n📅 {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}"
    
    return enhanced

def create_pre_commit_hook():
    """Create a pre-commit hook that updates dashboard status"""
    hook_content = """#!/bin/bash
# Pre-commit hook: Update dashboard status and run tests

echo "🔄 Running pre-commit dashboard integration..."

# Update documentation with latest dashboard status
python3 python-client/ai-portal.py --cmd docs --params '{"include": "status"}' 2>/dev/null || echo "⚠️ Could not update docs"

# Run dashboard tests if available
if [ -f "python-client/tests/test_ai_dashboard.py" ]; then
    echo "🧪 Running dashboard tests..."
    cd python-client && python -m pytest tests/test_ai_dashboard.py -v --tb=short || echo "⚠️ Dashboard tests failed"
    cd ..
fi

# Stage any documentation updates
git add README.md 2>/dev/null || true
git add python-client/README.md 2>/dev/null || true

echo "✅ Pre-commit dashboard integration complete"
"""
    
    hook_path = Path('.git/hooks/pre-commit')
    hook_path.parent.mkdir(exist_ok=True)
    hook_path.write_text(hook_content)
    hook_path.chmod(0o755)
    
    return hook_path

def create_commit_msg_hook():
    """Create a commit-msg hook that enhances commit messages"""
    hook_content = """#!/bin/bash
# Commit-msg hook: Enhance commit message with dashboard status

COMMIT_MSG_FILE=$1
COMMIT_MSG=$(cat $COMMIT_MSG_FILE)

# Skip if already enhanced or if it's a merge commit
if [[ "$COMMIT_MSG" == *"🤖 Generated with dashboard integration"* ]] || [[ "$COMMIT_MSG" == *"Merge"* ]]; then
    exit 0
fi

# Run dashboard integration enhancement
python3 python-client/git-dashboard-integration.py --enhance-commit "$COMMIT_MSG_FILE" 2>/dev/null || echo "⚠️ Could not enhance commit message"
"""
    
    hook_path = Path('.git/hooks/commit-msg')
    hook_path.parent.mkdir(exist_ok=True)
    hook_path.write_text(hook_content)
    hook_path.chmod(0o755)
    
    return hook_path

def install_git_hooks():
    """Install git hooks for dashboard integration"""
    if not Path('.git').exists():
        print("❌ Not a git repository")
        return False
    
    try:
        pre_commit_hook = create_pre_commit_hook()
        commit_msg_hook = create_commit_msg_hook()
        
        print(f"✅ Installed git hooks:")
        print(f"   📝 Pre-commit: {pre_commit_hook}")
        print(f"   💬 Commit-msg: {commit_msg_hook}")
        print(f"\n🔄 Hooks will now:")
        print(f"   • Update dashboard status before commits")
        print(f"   • Run dashboard tests during pre-commit")
        print(f"   • Enhance commit messages with project health")
        print(f"   • Auto-stage documentation updates")
        
        return True
    except Exception as e:
        print(f"❌ Failed to install git hooks: {e}")
        return False

def enhance_commit_file(commit_file_path):
    """Enhance a commit message file with dashboard status"""
    try:
        commit_file = Path(commit_file_path)
        original_msg = commit_file.read_text().strip()
        
        # Skip if already enhanced
        if "🤖 Generated with dashboard integration" in original_msg:
            return
        
        # Get dashboard and test status
        dashboard_info = get_dashboard_status()
        test_results = run_continuum_tests()
        
        # Enhance the message
        enhanced_msg = enhance_commit_message(original_msg, dashboard_info, test_results)
        
        # Write back to file
        commit_file.write_text(enhanced_msg)
        
    except Exception as e:
        print(f"⚠️ Could not enhance commit message: {e}", file=sys.stderr)

def main():
    """Main CLI interface"""
    if len(sys.argv) < 2:
        print("Git Dashboard Integration Tool")
        print("\nUsage:")
        print("  python3 git-dashboard-integration.py --install-hooks")
        print("  python3 git-dashboard-integration.py --enhance-commit <file>")
        print("  python3 git-dashboard-integration.py --status")
        print("  python3 git-dashboard-integration.py --test")
        return
    
    command = sys.argv[1]
    
    if command == '--install-hooks':
        install_git_hooks()
        
    elif command == '--enhance-commit' and len(sys.argv) > 2:
        enhance_commit_file(sys.argv[2])
        
    elif command == '--status':
        print("📊 Dashboard Integration Status:")
        
        # Check git hooks
        pre_commit_exists = Path('.git/hooks/pre-commit').exists()
        commit_msg_exists = Path('.git/hooks/commit-msg').exists()
        print(f"   📝 Pre-commit hook: {'✅ Installed' if pre_commit_exists else '❌ Missing'}")
        print(f"   💬 Commit-msg hook: {'✅ Installed' if commit_msg_exists else '❌ Missing'}")
        
        # Check dashboard status
        broken_count, _ = get_dashboard_status()
        if broken_count is not None:
            print(f"   🔧 Broken commands: {broken_count}")
        else:
            print(f"   🔧 Dashboard: ❌ Unavailable")
        
        # Check test integration
        test_passed, _, _ = run_continuum_tests()
        print(f"   🧪 Tests: {'✅ Passing' if test_passed else '❌ Failing'}")
        
    elif command == '--test':
        print("🧪 Testing git dashboard integration...")
        
        # Test dashboard access
        broken_count, dashboard_output = get_dashboard_status()
        if broken_count is not None:
            print(f"✅ Dashboard accessible: {broken_count} broken commands")
        else:
            print("❌ Dashboard not accessible")
        
        # Test continuum tests
        test_passed, test_output, test_error = run_continuum_tests()
        if test_passed:
            print("✅ Continuum tests accessible and passing")
        else:
            print(f"❌ Continuum tests failed: {test_error[:100] if test_error else 'Unknown error'}")
        
        # Test git integration
        git_status = get_git_status()
        if 'error' not in git_status:
            print(f"✅ Git integration working on branch: {git_status['branch']}")
        else:
            print(f"❌ Git integration error: {git_status['error']}")
        
    else:
        print(f"❌ Unknown command: {command}")

if __name__ == '__main__':
    main()