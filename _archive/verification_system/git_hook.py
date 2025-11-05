#!/usr/bin/env python3
"""
Continuum Git Hook - Clean, Modular Verification

A properly structured git hook that uses the verification_system package
for organized, testable verification with RunArtifact integration.
"""

import sys
import time
from pathlib import Path

# Add verification system to path
sys.path.insert(0, str(Path(__file__).parent))

from src.git_hook_verification import GitHookVerification
from src.run_artifact_integration import RunArtifactIntegration
from src.verification_history import VerificationHistory


def log_milestone(phase: str, action: str, details: str = ""):
    """Log major process milestone for UI progress tracking"""
    timestamp = time.strftime("%H:%M:%S")
    print(f"🎯 MILESTONE [{timestamp}] {phase}: {action}")
    if details:
        print(f"   ℹ️  {details}")


def main():
    """Main git hook verification process"""
    log_milestone("COMMIT_VERIFICATION_START", "Starting modular verification system")
    start_time = time.time()
    
    # Initialize verification components
    verification = GitHookVerification()
    run_artifact = RunArtifactIntegration()
    history = VerificationHistory()
    
    try:
        # Run verification process
        log_milestone("VERIFICATION_PROCESS", "Executing verification subprocess")
        success, verification_data, screenshot_path = verification.run_full_verification()
        
        elapsed = time.time() - start_time
        commit_sha = verification.get_commit_sha()
        
        if success and not verification_data.get('skipped'):
            log_milestone("VERIFICATION_SUCCESS", f"Verification passed ({elapsed:.1f}s)")
            
            # Create RunArtifact structure
            log_milestone("RUNARTIFACT_CREATION", "Creating RunArtifact structure")
            run_dir = run_artifact.create_full_artifact(commit_sha, verification_data, screenshot_path)
            log_milestone("RUNARTIFACT_COMPLETE", f"RunArtifact created: {run_dir}")
            
            # Update verification history
            log_milestone("HISTORY_UPDATE", "Updating verification history")
            history.add_entry("PASS", elapsed, f"run_{commit_sha}")
            
            # Stage verification files
            log_milestone("GIT_STAGING", "Staging verification files")
            import subprocess
            
            # Stage RunArtifact files
            subprocess.run(['git', 'add', '-A', '.continuum/verification/'], check=True)
            
            # Stage history file
            subprocess.run(['git', 'add', 'verification/history.txt'], check=True)
            
            log_milestone("SUCCESS", f"Verification complete ({elapsed:.1f}s)")
            print(f"✅ PASSED ({elapsed:.1f}s) - RunArtifact: {run_dir.name}")
            
            sys.exit(0)
        
        elif verification_data.get('skipped'):
            log_milestone("VERIFICATION_SKIPPED", "Verification skipped for this commit type")
            sys.exit(0)
        
        else:
            # Verification failed
            log_milestone("VERIFICATION_FAILED", f"Verification failed ({elapsed:.1f}s)")
            
            # Still update history for failure tracking
            history.add_entry("FAIL", elapsed)
            subprocess.run(['git', 'add', 'verification/history.txt'], check=True)
            
            print(f"❌ FAILED ({elapsed:.1f}s)")
            if 'error' in verification_data:
                print(f"Error: {verification_data['error']}")
            
            sys.exit(1)
    
    except Exception as e:
        elapsed = time.time() - start_time
        log_milestone("VERIFICATION_ERROR", f"Unexpected error ({elapsed:.1f}s)")
        print(f"❌ ERROR ({elapsed:.1f}s): {str(e)}")
        sys.exit(1)


if __name__ == "__main__":
    main()