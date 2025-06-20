#!/usr/bin/env python3
"""
Elegant commit verification with single file output
"""

import subprocess
import sys
import time
from pathlib import Path

def run_verification():
    """Run verification and return result"""
    return subprocess.run([
        sys.executable, 'devtools_full_demo.py', '--emergency-only'
    ], capture_output=True, text=True, timeout=30)

def create_verification_proof(screenshot_path):
    """Create single verification file"""
    timestamp = time.strftime("%Y%m%d_%H%M%S")
    proof_dir = Path('verification/ui-captures/')
    proof_dir.mkdir(parents=True, exist_ok=True)
    
    # Clean up any existing verification files
    for old_file in proof_dir.glob('ui-capture-*.jpg'):
        old_file.unlink(missing_ok=True)
    
    proof_path = proof_dir / f"ui-capture-{timestamp}.jpg"
    
    # Create compressed proof
    subprocess.run([
        'sips', '-Z', '640', '-s', 'formatOptions', '50',
        str(screenshot_path), '--out', str(proof_path)
    ], capture_output=True)
    
    return proof_path

def main():
    # Skip verification commits to prevent recursion
    try:
        commit_msg = Path('.git/COMMIT_EDITMSG').read_text()
        if any(word in commit_msg.lower() for word in ['verification', 'screenshot', 'test commit']):
            sys.exit(0)
    except:
        pass
    
    print("🚨 COMMIT VERIFICATION")
    start_time = time.time()
    
    # Run verification
    result = run_verification()
    elapsed = time.time() - start_time
    
    if (result.returncode == 0 and 
        'BIDIRECTIONAL FEEDBACK VERIFIED' in result.stdout):
        
        # Find screenshot
        screenshots = list(Path('.continuum/screenshots/').glob('agent_feedback_*.png'))
        if screenshots:
            latest_screenshot = max(screenshots, key=lambda p: p.stat().st_mtime)
            proof_path = create_verification_proof(latest_screenshot)
            
            # Stage single verification file
            subprocess.run(['git', 'add', str(proof_path)], check=True)
            
            print(f"✅ PASSED ({elapsed:.1f}s) - {proof_path.name}")
            sys.exit(0)
    
    print(f"❌ FAILED ({elapsed:.1f}s)")
    sys.exit(1)

if __name__ == "__main__":
    main()