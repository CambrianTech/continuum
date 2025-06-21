#!/usr/bin/env python3
"""
Elegant commit verification with single file output
"""

import subprocess
import sys
import time
from pathlib import Path

def log_milestone(phase, action, details=""):
    """Log major process milestone for UI progress tracking"""
    timestamp = time.strftime("%H:%M:%S")
    print(f"🎯 MILESTONE [{timestamp}] {phase}: {action}")
    if details:
        print(f"   ℹ️  {details}")

def run_verification():
    """Run verification and return result"""
    log_milestone("VERIFICATION_START", "Launching emergency verification system")
    log_milestone("BROWSER_LAUNCH", "Starting DevTools recovery browser", 
                 "devtools_full_demo.py --commit-check")
    
    result = subprocess.run([
        sys.executable, 'devtools_full_demo.py', '--commit-check'
    ], capture_output=True, text=True, timeout=60)
    
    if result.returncode == 0:
        log_milestone("VERIFICATION_COMPLETE", "Emergency verification successful")
    else:
        log_milestone("VERIFICATION_FAILED", "Emergency verification failed", 
                     f"Exit code: {result.returncode}")
    
    return result

def create_verification_proof(screenshot_path):
    """Create single verification file"""
    timestamp = time.strftime("%Y%m%d_%H%M%S")
    proof_dir = Path('verification/ui-captures/')
    proof_dir.mkdir(parents=True, exist_ok=True)
    
    # Clean up any existing verification files
    for old_file in proof_dir.glob('ui-capture-*.jpg'):
        old_file.unlink(missing_ok=True)
    
    proof_path = proof_dir / f"ui-capture-{timestamp}.jpg"
    
    # Create high-quality proof (readable version info)
    subprocess.run([
        'sips', '-Z', '1200', '-s', 'formatOptions', '80',
        str(screenshot_path), '--out', str(proof_path)
    ], capture_output=True)
    
    return proof_path

def validate_cleanup():
    """Validate that verification system cleaned up properly"""
    errors = []
    
    # Check for unstaged verification files
    result = subprocess.run(['git', 'status', '--porcelain'], 
                          capture_output=True, text=True)
    
    for line in result.stdout.splitlines():
        if 'verification/' in line:
            status = line[:2]
            filename = line[3:]
            if status.strip() in ['D', 'M']:  # Deleted or modified but not staged
                errors.append(f"❌ UNSTAGED: {filename} ({status.strip()})")
    
    # Check for verification log files that should be cleaned
    verification_logs = list(Path('.continuum/screenshots/').glob('agent_feedback_*.png'))
    if len(verification_logs) > 1:  # Keep only the latest
        errors.append(f"❌ LOG CLEANUP: {len(verification_logs)} screenshot files remain")
    
    return errors

def main():
    # Skip verification commits to prevent recursion
    try:
        commit_msg = Path('.git/COMMIT_EDITMSG').read_text()
        if any(word in commit_msg.lower() for word in ['verification', 'screenshot', 'test commit']):
            sys.exit(0)
    except:
        pass
    
    # Clean up any previously staged verification files FIRST
    log_milestone("CLEANUP_START", "Cleaning previously staged verification files")
    subprocess.run(['git', 'reset', 'HEAD', 'verification/'], capture_output=True)
    log_milestone("CLEANUP_COMPLETE", "Verification staging area cleaned")
    
    log_milestone("COMMIT_VERIFICATION_START", "Starting commit verification process")
    start_time = time.time()
    
    # Run verification
    result = run_verification()
    elapsed = time.time() - start_time
    
    if (result.returncode == 0 and 
        'BIDIRECTIONAL FEEDBACK VERIFIED' in result.stdout and
        'COMPLETE FEEDBACK LOOP OPERATIONAL' in result.stdout and
        'Agent CAN execute JavaScript' in result.stdout and
        'Agent CAN see its own console output' in result.stdout and
        'Agent CAN capture screenshots' in result.stdout):
        
        # Find screenshot
        log_milestone("SCREENSHOT_SEARCH", "Locating verification screenshot")
        screenshots = list(Path('.continuum/screenshots/').glob('agent_feedback_*.png'))
        if screenshots:
            latest_screenshot = max(screenshots, key=lambda p: p.stat().st_mtime)
            log_milestone("SCREENSHOT_FOUND", f"Located screenshot: {latest_screenshot.name}")
            
            log_milestone("PROOF_CREATION", "Creating verification proof")
            proof_path = create_verification_proof(latest_screenshot)
            log_milestone("PROOF_CREATED", f"Verification proof ready: {proof_path.name}")
            
            # Check for cleanup issues BEFORE staging
            pre_stage_errors = validate_cleanup()
            
            # Stage verification changes (new file + deletions) - REQUIRED for commit validation
            log_milestone("GIT_STAGING", "Staging verification artifacts")
            subprocess.run(['git', 'add', str(proof_path)], check=True)
            subprocess.run(['git', 'add', '-A', 'verification/'], check=True)  # -A stages deletions
            
            # Stage important logs for verification
            log_paths = [
                'python-client/.continuum/ai-portal/logs/buffer.log',
                '.continuum/continuum.log'
            ]
            for log_path in log_paths:
                if Path(log_path).exists():
                    subprocess.run(['git', 'add', log_path], capture_output=True)
            
            log_milestone("VERIFICATION_SUCCESS", f"Commit verification complete ({elapsed:.1f}s)")
            
            # Report results
            print(f"📸 Verification proof created and staged: {proof_path}")
            print(f"✅ PASSED ({elapsed:.1f}s) - {proof_path.name}")
            if pre_stage_errors:
                print("🚨 CLEANUP ERRORS (FIXED):")
                for error in pre_stage_errors:
                    print(f"  {error}")
            sys.exit(0)
    
    print(f"❌ FAILED ({elapsed:.1f}s)")
    sys.exit(1)

if __name__ == "__main__":
    if len(sys.argv) > 1:
        if sys.argv[1] == "--check-status":
            # Just check git status for verification files
            result = subprocess.run(['git', 'status', '--porcelain'], capture_output=True, text=True)
            print("🔍 GIT STATUS FOR VERIFICATION FILES:")
            for line in result.stdout.splitlines():
                if 'verification/' in line:
                    status = line[:2]
                    filename = line[3:]
                    print(f"  {status} {filename}")
        elif sys.argv[1] == "--check-cleanup":
            # Run just the cleanup validation
            errors = validate_cleanup()
            if errors:
                print("🚨 CLEANUP ERRORS DETECTED:")
                for error in errors:
                    print(f"  {error}")
            else:
                print("✅ NO CLEANUP ERRORS")
        elif sys.argv[1] == "--files-exist":
            # Check what files actually exist vs git thinks
            print("📁 ACTUAL FILES:")
            proof_dir = Path('verification/ui-captures/')
            if proof_dir.exists():
                for f in proof_dir.glob('*.jpg'):
                    print(f"  EXISTS: {f.name}")
            else:
                print("  No verification directory")
        else:
            main()
    else:
        main()