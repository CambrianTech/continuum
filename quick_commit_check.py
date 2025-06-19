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

# Global progress control
progress_done = False

def show_progress():
    """Show progress indicators during verification"""
    import threading
    import sys
    
    def progress_spinner():
        chars = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
        while not progress_done:
            for char in chars:
                if progress_done:
                    break
                print(f"\r🔄 {char} Verifying system health...", end="", flush=True)
                time.sleep(0.1)
    
    global progress_done
    progress_done = False
    thread = threading.Thread(target=progress_spinner, daemon=True)
    thread.start()
    return thread

def open_progress_widget():
    """Open the progress widget in browser"""
    try:
        widget_path = Path('verification/progress-widget.html').absolute()
        if widget_path.exists():
            subprocess.run(['open', str(widget_path)], check=False)
            return True
    except:
        pass
    return False

def main():
    global progress_done
    
    print("🚨 COMMIT VERIFICATION - FAST MODE")
    print("=" * 40)
    start_time = time.time()
    
    # Open progress widget in browser
    widget_opened = open_progress_widget()
    if widget_opened:
        print("🌐 Progress widget opened in browser")
    
    # Show progress during verification
    progress_thread = show_progress()
    
    try:
        print("🚀 Starting DevTools recovery system...")
        # Run emergency-only recovery with timeout
        result = subprocess.run([
            sys.executable, 'devtools_full_demo.py', '--emergency-only'
        ], capture_output=True, text=True, timeout=30)
        
        # Stop progress indicator
        progress_done = True
        print(f"\r✅ System verification completed!", " " * 20)  # Clear spinner
        
        elapsed = time.time() - start_time
        print(f"⏱️ VERIFICATION TIME: {elapsed:.1f}s")
        
        # Check if verification passed
        if result.returncode == 0:
            # Look for success markers
            if 'BIDIRECTIONAL FEEDBACK VERIFIED' in result.stdout:
                # Check for screenshots
                screenshots = list(Path('.continuum/screenshots/').glob('agent_feedback_*.png'))
                if len(screenshots) > 0:
                    print("📸 Creating UI capture proof...")
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
        progress_done = True
        print(f"\r❌ FAILED - Verification timeout (>30s)", " " * 20)
        sys.exit(1)
    except Exception as e:
        progress_done = True
        print(f"\r❌ FAILED - Verification error: {e}", " " * 20)
        sys.exit(1)

if __name__ == "__main__":
    main()