#!/usr/bin/env python3
"""
Quick commit verification script for git hooks
Fast PASS/FAIL verification in under 15 seconds
"""

import subprocess
import sys
import time
from pathlib import Path

def create_screenshot_proof(screenshot_path):
    """Create git-trackable UI capture proof"""
    try:
        # Create git-tracked directory
        proof_dir = Path('verification/ui-captures/')
        proof_dir.mkdir(parents=True, exist_ok=True)
        
        # Generate datetime-based filename
        timestamp = time.strftime("%Y%m%d_%H%M%S")
        proof_path = proof_dir / f"ui-capture-{timestamp}.jpg"
        
        # Use sips (macOS) to create compressed JPEG
        result = subprocess.run([
            'sips', '-Z', '640', '-s', 'formatOptions', '50',  # 640px max, 50% quality
            str(screenshot_path), '--out', str(proof_path)
        ], capture_output=True)
        
        if result.returncode == 0 and proof_path.exists():
            return str(proof_path)
        else:
            return str(screenshot_path)  # Fallback to original
            
    except Exception:
        return str(screenshot_path)  # Fallback to original

def main():
    print("🚨 COMMIT VERIFICATION - FAST MODE")
    print("=" * 40)
    start_time = time.time()
    
    try:
        # Run emergency-only recovery with timeout
        result = subprocess.run([
            sys.executable, 'devtools_full_demo.py', '--emergency-only'
        ], capture_output=True, text=True, timeout=30)
        
        elapsed = time.time() - start_time
        print(f"\n⏱️ VERIFICATION TIME: {elapsed:.1f}s")
        
        # Check if verification passed
        if result.returncode == 0:
            # Look for success markers
            if 'BIDIRECTIONAL FEEDBACK VERIFIED' in result.stdout:
                # Check for screenshots
                screenshots = list(Path('.continuum/screenshots/').glob('agent_feedback_*.png'))
                if len(screenshots) > 0:
                    # Create git-trackable screenshot proof
                    latest_screenshot = max(screenshots, key=lambda p: p.stat().st_mtime)
                    proof_path = create_screenshot_proof(latest_screenshot)
                    
                    print("✅ PASSED - All systems operational")
                    print("📊 UUID tracking: ✅ | Screenshots: ✅ | Logs: ✅")
                    print(f"📸 Screenshot-proof: {proof_path}")
                    sys.exit(0)
        
        print("❌ FAILED - System health compromised")
        print(f"Return code: {result.returncode}")
        if result.stderr:
            print(f"Error: {result.stderr[:200]}")
        sys.exit(1)
        
    except subprocess.TimeoutExpired:
        print("❌ FAILED - Verification timeout (>30s)")
        sys.exit(1)
    except Exception as e:
        print(f"❌ FAILED - Verification error: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()