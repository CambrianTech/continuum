#!/usr/bin/env python3
"""
Quick commit verification script for git hooks
Fast PASS/FAIL verification in under 15 seconds
Fixed: Single verification screenshot per commit
"""

import subprocess
import sys
import time
from pathlib import Path

def cleanup_old_verification_files():
    """Clean up all old verification files"""
    verification_dir = Path('verification/ui-captures/')
    if verification_dir.exists():
        all_captures = list(verification_dir.glob('ui-capture-*.jpg'))
        for old_capture in all_captures:
            try:
                subprocess.run(['git', 'reset', 'HEAD', str(old_capture)], 
                             capture_output=True, stderr=subprocess.DEVNULL)
                old_capture.unlink(missing_ok=True)
                print(f"🗑️ Removed: {old_capture.name}")
            except Exception:
                pass

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

def run_verification():
    """Run the actual verification process"""
    print("🚀 Starting DevTools recovery system...")
    result = subprocess.run([
        sys.executable, 'devtools_full_demo.py', '--emergency-only'
    ], capture_output=True, text=True, timeout=30)
    
    return result

def save_and_stage_verification_logs(timestamp):
    """Save and stage verification logs"""
    try:
        logs_dir = Path('verification/logs/')
        logs_dir.mkdir(parents=True, exist_ok=True)
        
        # Save client logs
        client_log_path = logs_dir / f"client-{timestamp}.log"
        result = subprocess.run([
            sys.executable, 'python-client/ai-portal.py', '--logs', '20'
        ], capture_output=True, text=True, timeout=10)
        
        if result.returncode == 0:
            with open(client_log_path, 'w') as f:
                f.write(f"=== CLIENT LOGS CAPTURED DURING VERIFICATION ===\n")
                f.write(f"Timestamp: {timestamp}\n\n")
                f.write(result.stdout)
            
            # Stage the log file
            subprocess.run(['git', 'add', str(client_log_path)], check=True)
            print(f"✅ Added client log: {client_log_path.name}")
            return [str(client_log_path)]
        
        return []
    except Exception as e:
        print(f"⚠️ Could not save logs: {e}")
        return []

def stage_verification_files(proof_path, log_files):
    """Stage verification screenshot and logs"""
    try:
        subprocess.run(['git', 'add', proof_path], check=True)
        print(f"✅ Added verification screenshot: {Path(proof_path).name}")
        
        for log_file in log_files:
            print(f"✅ Added log file: {Path(log_file).name}")
        
        return True
    except Exception as e:
        print(f"⚠️ Could not stage files: {e}")
        return False

def main():
    # Prevent multiple executions
    lockfile = Path('/tmp/continuum_verification.lock')
    if lockfile.exists():
        print("🔒 Verification already running - skipping")
        sys.exit(0)
    
    try:
        lockfile.touch()
        
        print("🚨 COMMIT VERIFICATION - FAST MODE")
        print("=" * 40)
        start_time = time.time()
        
        # Clean up old verification files
        print("🧹 Cleaning up old verification files...")
        cleanup_old_verification_files()
        
        # Run verification
        result = run_verification()
        
        elapsed = time.time() - start_time
        print(f"⏱️ VERIFICATION TIME: {elapsed:.1f}s")
        
        # Check if verification passed
        if result.returncode == 0:
            if 'BIDIRECTIONAL FEEDBACK VERIFIED' in result.stdout:
                # Check for screenshots
                screenshots = list(Path('.continuum/screenshots/').glob('agent_feedback_*.png'))
                if len(screenshots) > 0:
                    print("📸 Creating UI capture proof...")
                    latest_screenshot = max(screenshots, key=lambda p: p.stat().st_mtime)
                    proof_path = create_screenshot_proof(latest_screenshot)
                    
                    # Generate timestamp for logs
                    timestamp = time.strftime("%Y%m%d_%H%M%S")
                    
                    # Save and stage logs
                    print("📝 Saving verification logs...")
                    log_files = save_and_stage_verification_logs(timestamp)
                    
                    print("✅ PASSED - All systems operational")
                    print(f"📸 Screenshot-proof: {proof_path}")
                    
                    # Stage the verification files
                    if stage_verification_files(proof_path, log_files):
                        sys.exit(0)
        
        print("❌ FAILED - System health compromised")
        sys.exit(1)
        
    except subprocess.TimeoutExpired:
        print("❌ FAILED - Verification timeout (>30s)")
        sys.exit(1)
    except Exception as e:
        print(f"❌ FAILED - Verification error: {e}")
        sys.exit(1)
    finally:
        # Always clean up lockfile
        lockfile.unlink(missing_ok=True)

if __name__ == "__main__":
    main()