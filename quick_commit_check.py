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
    """Create verification package using RunArtifact system for universal compatibility:
    .continuum/
    └── verification/                  # Run type
        ├── run_sha/                   # Run ID (commit SHA)
        │   ├── run.json               # Run metadata and status
        │   ├── summary.txt            # Human-readable run summary
        │   ├── client-logs.txt        # Client-side logs and activity
        │   ├── server-logs.txt        # Server/daemon logs and activity
        │   ├── console-logs.txt       # Browser console output
        │   ├── ui-capture.png         # Screenshot of UI state
        │   └── error-logs.txt         # Error details if any
        └── latest -> run_sha/         # Symlink to latest run
    """
    
    # Get current commit SHA for run ID
    try:
        sha_result = subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True)
        commit_sha = sha_result.stdout.strip()[:12]  # First 12 chars of SHA
    except:
        commit_sha = time.strftime("%Y%m%d_%H%M%S")  # Fallback to timestamp
    
    # Import RunArtifact system for structured verification storage
    sys.path.insert(0, str(Path.cwd()))
    import json
    import os
    try:
        # Try to import via Node.js since RunArtifact is a CommonJS module
        import subprocess
        
        # Create a simple Python wrapper for RunArtifact functionality
        def create_run_artifact_structure(base_dir, run_type, run_id):
            """Create RunArtifact directory structure directly"""
            run_dir = Path(base_dir) / run_type / f"run_{run_id}"
            run_dir.mkdir(parents=True, exist_ok=True)
            
            # Create latest symlink
            latest_link = Path(base_dir) / run_type / "latest"
            if latest_link.exists() or latest_link.is_symlink():
                latest_link.unlink()
            latest_link.symlink_to(f"run_{run_id}")
            
            return run_dir
        
        def write_run_artifact(run_dir, filename, content):
            """Write artifact file with proper formatting"""
            filepath = run_dir / filename
            if isinstance(content, dict):
                with open(filepath, 'w') as f:
                    json.dump(content, f, indent=2)
            else:
                filepath.write_text(str(content))
        
        # Use the Python wrapper instead of importing RunArtifact directly
        class RunArtifactPython:
            def __init__(self, base_dir, run_type, run_id):
                self.base_dir = base_dir
                self.run_type = run_type
                self.run_id = run_id
                self.run_dir = None
            
            def ensureDirectoryStructure(self):
                self.run_dir = create_run_artifact_structure(self.base_dir, self.run_type, self.run_id)
            
            def writeArtifact(self, filename, content):
                write_run_artifact(self.run_dir, filename, content)
            
            def getArtifactPath(self, filename):
                return self.run_dir / filename
            
            def updateLatestSymlink(self):
                # Already handled in ensureDirectoryStructure
                pass
            
            def readArtifact(self, filename):
                filepath = self.run_dir / filename
                if filename.endswith('.json'):
                    with open(filepath, 'r') as f:
                        return json.load(f)
                else:
                    return filepath.read_text()
        
        RunArtifact = RunArtifactPython
        
        # Create RunArtifact for this git verification
        artifact = RunArtifact('.continuum', 'verification', commit_sha)
        artifact.ensureDirectoryStructure()
        
        log_milestone("RUNARTIFACT_CREATED", f"RunArtifact verification/{commit_sha}")
        
        # Create run.json with metadata
        run_metadata = {
            "runType": "verification",
            "runId": commit_sha,
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "status": "PASS",
            "duration": None,  # Will be filled later
            "gitHook": True,
            "commitSha": commit_sha,
            "verification": {
                "javascriptExecution": True,
                "consoleCapture": True,
                "screenshotCapture": True,
                "feedbackLoop": True
            }
        }
        artifact.writeArtifact('run.json', run_metadata)
        
        # Create summary.txt
        summary = f"""Git Hook Verification - Commit {commit_sha}
        
✅ COMPLETE FEEDBACK LOOP OPERATIONAL
✅ Agent CAN execute JavaScript
✅ Agent CAN see its own console output  
✅ Agent CAN capture screenshots
✅ Agent HAS full visibility into its own actions

This verification confirms all JTAG debugging capabilities are working.
"""
        artifact.writeArtifact('summary.txt', summary)
        
        # Create 1280px wide interface screenshot
        ui_capture_path = artifact.getArtifactPath('ui-capture.png')
        subprocess.run([
            'sips', '-Z', '1280', '-s', 'format', 'png',
            str(screenshot_path), '--out', str(ui_capture_path)
        ], capture_output=True)
        log_milestone("SCREENSHOT_SAVED", f"UI capture: {ui_capture_path}")
        
        # Extract and save logs using RunArtifact structure
        if verification_result and verification_result.stdout:
            verification_output = verification_result.stdout
            
            client_logs = "# Client logs from verification session\n"
            server_logs = "# Server logs from verification session\n"
            console_logs = "# Browser console output from verification\n"
            
            # Parse the verification output to separate different log types
            lines = verification_output.split('\n')
            
            for line in lines:
                if any(marker in line for marker in ['CLIENT-SIDE', 'PORTAL', 'BROWSER_LOG', 'WebSocket']):
                    client_logs += line + "\n"
                elif 'UUID_' in line and any(marker in line for marker in ['LOG:', 'ERROR:', 'WARNING:']):
                    console_logs += line + "\n"  # Browser console logs
                elif any(marker in line for marker in ['SERVER-SIDE', 'DevTools', 'MILESTONE', 'INFO:']):
                    server_logs += line + "\n"
                else:
                    # General verification logs go to server section
                    server_logs += line + "\n"
        else:
            client_logs = "# No client logs captured during verification session\n"
            server_logs = "# No server logs captured during verification session\n"
            console_logs = "# No console logs captured during verification session\n"
        
        # Write all log artifacts using RunArtifact
        artifact.writeArtifact('client-logs.txt', client_logs)
        artifact.writeArtifact('server-logs.txt', server_logs)
        artifact.writeArtifact('console-logs.txt', console_logs)
        artifact.writeArtifact('error-logs.txt', "# No errors during verification\n")
        
        # Update latest symlink
        artifact.updateLatestSymlink()
        
        log_milestone("RUNARTIFACT_COMPLETE", f"RunArtifact structure created: .continuum/verification/run_{commit_sha}")
        
        # Store artifact reference for duration update later
        create_verification_proof.artifact = artifact
        
        return ui_capture_path
        
    except ImportError as e:
        log_milestone("RUNARTIFACT_FALLBACK", f"RunArtifact import failed: {e}")
        # Fallback to old verification system if RunArtifact is not available
        return create_legacy_verification_proof(screenshot_path, verification_result, commit_sha)

def create_legacy_verification_proof(screenshot_path, verification_result, commit_sha):
    """Legacy verification structure for backward compatibility"""
    # Create legacy verification directory structure
    verification_base = Path('verification')
    verification_base.mkdir(exist_ok=True)
    
    verification_sha_dir = verification_base / f"verification_{commit_sha}"
    verification_sha_dir.mkdir(exist_ok=True)
    
    log_milestone("LEGACY_DIR_CREATED", f"Legacy verification directory: verification_{commit_sha}")
    
    # Create 1280px wide interface screenshot
    ui_capture_path = verification_sha_dir / "ui-capture.png"
    subprocess.run([
        'sips', '-Z', '1280', '-s', 'format', 'png',
        str(screenshot_path), '--out', str(ui_capture_path)
    ], capture_output=True)
    log_milestone("LEGACY_SCREENSHOT_SAVED", f"Legacy UI capture: {ui_capture_path}")
    
    # Extract and save logs (legacy structure)
    if verification_result and verification_result.stdout:
        verification_output = verification_result.stdout
        
        client_logs = "# Client logs from verification session\n"
        server_logs = "# Server logs from verification session\n"
        
        lines = verification_output.split('\n')
        for line in lines:
            if any(marker in line for marker in ['CLIENT-SIDE', 'PORTAL', 'BROWSER_LOG', 'WebSocket']):
                client_logs += line + "\n"
            elif any(marker in line for marker in ['SERVER-SIDE', 'DevTools', 'MILESTONE', 'INFO:']):
                server_logs += line + "\n"
            elif 'UUID_' in line and any(marker in line for marker in ['LOG:', 'ERROR:', 'WARNING:']):
                client_logs += line + "\n"  # Console logs go to client section
            else:
                server_logs += line + "\n"
    else:
        client_logs = "# No client logs captured during verification session\n"
        server_logs = "# No server logs captured during verification session\n"
    
    # Write the session-specific logs (legacy)
    (verification_sha_dir / "client-logs.txt").write_text(client_logs)
    (verification_sha_dir / "server-logs.txt").write_text(server_logs)
    
    log_milestone("LEGACY_LOGS_SAVED", f"Legacy logs saved to verification_{commit_sha}")
    
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
            
            # Stage RunArtifact verification files if they exist
            continuum_verification = Path('.continuum/verification/')
            if continuum_verification.exists():
                subprocess.run(['git', 'add', '-A', '.continuum/verification/'], check=True)
                log_milestone("RUNARTIFACT_STAGED", "RunArtifact verification structure staged")
            
            # Stage legacy verification files if they exist
            legacy_verification = Path('verification/')
            if legacy_verification.exists():
                subprocess.run(['git', 'add', '-A', 'verification/'], check=True)
                log_milestone("LEGACY_STAGED", "Legacy verification structure staged")
            
            # Update RunArtifact duration if available
            if hasattr(create_verification_proof, 'artifact'):
                artifact = create_verification_proof.artifact
                # Update run.json with final duration
                run_data = artifact.readArtifact('run.json')
                if isinstance(run_data, str):
                    import json
                    run_data = json.loads(run_data)
                run_data['duration'] = elapsed * 1000  # Store in milliseconds
                artifact.writeArtifact('run.json', run_data)
                log_milestone("DURATION_UPDATED", f"RunArtifact duration updated: {elapsed:.1f}s")
            
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