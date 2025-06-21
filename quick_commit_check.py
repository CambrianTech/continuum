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

def create_verification_proof(screenshot_path, verification_result):
    """Create verification package in proper structure:
    verification/
    ├── history.txt                    # Summary of all verifications
    └── verification_sha/              # One dir per commit SHA
        ├── ui-capture.png             # 1280px wide interface screenshot
        ├── client-logs.txt            # Client logs from THIS verification session only
        └── server-logs.txt            # Server logs from THIS verification session only
    """
    
    # Get current commit SHA for directory naming
    try:
        sha_result = subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True)
        commit_sha = sha_result.stdout.strip()[:12]  # First 12 chars of SHA
    except:
        commit_sha = time.strftime("%Y%m%d_%H%M%S")  # Fallback to timestamp
    
    # Create verification directory structure
    verification_base = Path('verification')
    verification_base.mkdir(exist_ok=True)
    
    verification_sha_dir = verification_base / f"verification_{commit_sha}"
    verification_sha_dir.mkdir(exist_ok=True)
    
    log_milestone("DIR_CREATED", f"Verification directory: verification_{commit_sha}")
    
    # Create 1280px wide interface screenshot
    ui_capture_path = verification_sha_dir / "ui-capture.png"
    subprocess.run([
        'sips', '-Z', '1280', '-s', 'format', 'png',
        str(screenshot_path), '--out', str(ui_capture_path)
    ], capture_output=True)
    log_milestone("SCREENSHOT_SAVED", f"UI capture: {ui_capture_path}")
    
    # Save only the logs from THIS verification session
    client_log_dest = verification_sha_dir / "client-logs.txt"
    server_log_dest = verification_sha_dir / "server-logs.txt"
    
    # Extract relevant logs from verification result output
    if verification_result and verification_result.stdout:
        # Split verification output into client and server sections
        verification_output = verification_result.stdout
        
        client_logs = "# Client logs from verification session\n"
        server_logs = "# Server logs from verification session\n"
        
        # Parse the verification output to separate client vs server logs
        lines = verification_output.split('\n')
        current_section = "general"
        
        for line in lines:
            if any(marker in line for marker in ['CLIENT-SIDE', 'PORTAL', 'BROWSER_LOG', 'WebSocket']):
                client_logs += line + "\n"
            elif any(marker in line for marker in ['SERVER-SIDE', 'DevTools', 'MILESTONE', 'INFO:']):
                server_logs += line + "\n"
            elif 'UUID_' in line and any(marker in line for marker in ['LOG:', 'ERROR:', 'WARNING:']):
                client_logs += line + "\n"  # Console logs go to client section
            else:
                # General verification logs go to server section
                server_logs += line + "\n"
    else:
        client_logs = "# No client logs captured during verification session\n"
        server_logs = "# No server logs captured during verification session\n"
    
    # Write the session-specific logs
    client_log_dest.write_text(client_logs)
    server_log_dest.write_text(server_logs)
    
    log_milestone("CLIENT_LOGS", f"Verification session client logs saved: {client_log_dest}")
    log_milestone("SERVER_LOGS", f"Verification session server logs saved: {server_log_dest}")
    
    return ui_capture_path

def update_verification_history(status, elapsed_time, proof_path=None):
    """Update verification/history.txt with verification results"""
    
    # Get commit message and SHA
    try:
        # Get the commit message being attempted
        commit_msg_result = subprocess.run(['git', 'log', '-1', '--pretty=format:%s'], capture_output=True, text=True)
        commit_message = commit_msg_result.stdout.strip() or "No commit message"
        
        # Get current SHA
        sha_result = subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True)
        commit_sha = sha_result.stdout.strip()[:12]
    except:
        commit_message = "Unknown commit"
        commit_sha = "unknown"
    
    # Create history entry
    timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
    verification_dir = f"verification_{commit_sha}" if proof_path else "none"
    
    history_entry = f"{timestamp} | {status} | {elapsed_time:.1f}s | {commit_sha} | {commit_message} | {verification_dir}\n"
    
    # Append to history.txt
    history_path = Path('verification/history.txt')
    
    # Create header if file doesn't exist
    if not history_path.exists():
        header = "# Continuum Verification History\n"
        header += "# Format: DateTime | Status | Duration | SHA | CommitMessage | VerificationDir\n\n"
        history_path.write_text(header)
    
    # Append new entry
    with open(history_path, 'a') as f:
        f.write(history_entry)
    
    log_milestone("HISTORY_UPDATED", f"Added {status} entry to verification history")

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
            proof_path = create_verification_proof(latest_screenshot, result)
            log_milestone("PROOF_CREATED", f"Verification proof ready: {proof_path.name}")
            
            # Check for cleanup issues BEFORE staging
            pre_stage_errors = validate_cleanup()
            
            # Stage complete verification package (screenshot + logs + stats)
            log_milestone("GIT_STAGING", "Staging complete verification package")
            subprocess.run(['git', 'add', '-A', 'verification/'], check=True)  # -A stages all verification files including deletions
            
            # Update verification history
            update_verification_history("PASS", elapsed, proof_path)
            
            # Stage the updated history file
            subprocess.run(['git', 'add', 'verification/history.txt'], check=True)
            log_milestone("HISTORY_STAGED", "Verification history staged for commit")
            
            log_milestone("VERIFICATION_SUCCESS", f"Commit verification complete ({elapsed:.1f}s)")
            
            # Report results
            print(f"📸 Verification proof created and staged: {proof_path}")
            print(f"✅ PASSED ({elapsed:.1f}s) - {proof_path.name}")
            if pre_stage_errors:
                print("🚨 CLEANUP ERRORS (FIXED):")
                for error in pre_stage_errors:
                    print(f"  {error}")
            sys.exit(0)
    
    # Update verification history for failure
    update_verification_history("FAIL", elapsed)
    
    # Stage the updated history file even for failures
    subprocess.run(['git', 'add', 'verification/history.txt'], check=True)
    
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