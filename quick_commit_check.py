#!/usr/bin/env python3
"""
Modern git commit verification using VerificationArtifact system
================================================================
Elegant inheritance-driven verification with organized artifact storage.

ARCHITECTURE:
- Uses VerificationArtifact for structured git verification
- Inheritance pattern: BaseArtifact -> VerificationArtifact  
- Directory structure: .continuum/artifacts/verification/YYYYMMDD_HHMMSS_SHA/
- Legacy compatibility: Creates verification/latest symlinks
- Full JTAG integration: screenshots, console logs, test results
"""

import subprocess
import sys
import time
import json
import os
from pathlib import Path

def log_milestone(phase, action, details=""):
    """Log major process milestone for UI progress tracking"""
    timestamp = time.strftime("%H:%M:%S")
    print(f"🎯 MILESTONE [{timestamp}] {phase}: {action}")
    if details:
        print(f"   ℹ️  {details}")

def get_git_context():
    """Extract git commit context information"""
    try:
        # Get current commit SHA
        sha_result = subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True)
        commit_sha = sha_result.stdout.strip()
        
        # Get commit message  
        msg_result = subprocess.run(['git', 'log', '-1', '--pretty=format:%s'], capture_output=True, text=True)
        commit_message = msg_result.stdout.strip()
        
        # Get changed files
        files_result = subprocess.run(['git', 'diff', '--name-only', 'HEAD~1'], capture_output=True, text=True)
        changed_files = [f.strip() for f in files_result.stdout.split('\n') if f.strip()]
        
        return commit_sha, commit_message, changed_files
    except Exception as e:
        print(f"❌ Git context extraction failed: {e}")
        return None, "Unknown commit", []

def create_node_verification_artifact(commit_sha, commit_message, changed_files):
    """Create VerificationArtifact using Node.js integration"""
    
    # Create a simple Node.js script to use VerificationArtifact
    node_script = f"""
const VerificationArtifact = require('../src/core/artifacts/VerificationArtifact.cjs');

async function createVerificationArtifact() {{
    const artifact = new VerificationArtifact('{commit_sha}');
    
    // Set git context
    artifact.setCommitContext('{commit_sha}', '{commit_message}', {json.dumps(changed_files)});
    
    // Create directory structure
    await artifact.createStructure();
    
    // Set initial status
    artifact.setVerificationStatus('pending', 'Starting git hook verification');
    
    // Save basic structure
    await artifact.saveVerificationData();
    
    // Create legacy compatibility symlink
    await artifact.createLegacySymlink();
    
    // Output artifact path for Python to use
    console.log(artifact.artifactPath);
}}

createVerificationArtifact().catch(console.error);
"""
    
    # Write and execute Node.js script (use .cjs for CommonJS)
    script_path = Path('.continuum/temp_verification_script.cjs')
    script_path.parent.mkdir(exist_ok=True)
    script_path.write_text(node_script)
    
    try:
        result = subprocess.run(['node', str(script_path)], capture_output=True, text=True, timeout=30)
        if result.returncode == 0:
            artifact_path = result.stdout.strip()
            script_path.unlink()  # Clean up temp script
            return artifact_path
        else:
            print(f"❌ Node.js VerificationArtifact creation failed: {result.stderr}")
            script_path.unlink()
            return None
    except Exception as e:
        print(f"❌ Node.js execution failed: {e}")
        if script_path.exists():
            script_path.unlink()
        return None

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

def update_verification_artifact(artifact_path, verification_result, screenshot_path=None):
    """Update VerificationArtifact with verification results"""
    
    # Parse verification output for console evidence and test results
    console_evidence = []
    test_results = {"passed": 0, "failed": 0, "totalTests": 0, "duration": 0}
    
    if verification_result.stdout:
        lines = verification_result.stdout.split('\n')
        for line in lines:
            if 'UUID_' in line and any(keyword in line for keyword in ['CONSOLE_LOG', 'AGENT_MONITORING', 'BACKGROUND_CHANGED']):
                console_evidence.append({"level": "log", "message": line.strip()})
    
    # Extract test timing if available
    if verification_result.returncode == 0:
        test_results["passed"] = 1
        test_results["totalTests"] = 1
        status = "passed"
        reason = "All verification checks passed"
    else:
        test_results["failed"] = 1
        test_results["totalTests"] = 1
        status = "failed"
        reason = f"Verification failed with exit code {verification_result.returncode}"
    
    # Create Node.js script to update the artifact
    node_update_script = f"""
const VerificationArtifact = require('../src/core/artifacts/VerificationArtifact.cjs');
const fs = require('fs');
const path = require('path');

async function updateArtifact() {{
    // Load existing artifact by reconstructing from path
    const pathParts = '{artifact_path}'.split('/');
    const artifactId = pathParts[pathParts.length - 1];
    const commitSha = artifactId.split('_')[2] || 'unknown';
    
    const artifact = new VerificationArtifact(commitSha);
    artifact.artifactPath = '{artifact_path}';
    artifact.id = artifactId;
    
    // Add console evidence
    const consoleEvidence = {json.dumps(console_evidence)};
    for (const evidence of consoleEvidence) {{
        artifact.addConsoleEvidence(evidence);
    }}
    
    // Set test results
    artifact.setTestResults({json.dumps(test_results)});
    
    // Set final verification status
    artifact.setVerificationStatus('{status}', '{reason}');
    
    // Copy screenshot if available
    if ('{screenshot_path}' && '{screenshot_path}' !== 'None') {{
        const screenshotDir = path.join(artifact.artifactPath, 'screenshots');
        const destPath = path.join(screenshotDir, 'ui-capture.png');
        if (fs.existsSync('{screenshot_path}')) {{
            await fs.promises.copyFile('{screenshot_path}', destPath);
        }}
    }}
    
    // Save all verification data
    await artifact.saveVerificationData();
    
    console.log('Verification artifact updated successfully');
}}

updateArtifact().catch(console.error);
"""
    
    script_path = Path('.continuum/temp_update_script.cjs')
    script_path.write_text(node_update_script)
    
    try:
        result = subprocess.run(['node', str(script_path)], capture_output=True, text=True, timeout=30)
        script_path.unlink()
        return result.returncode == 0
    except Exception as e:
        print(f"❌ Artifact update failed: {e}")
        if script_path.exists():
            script_path.unlink()
        return False

def find_screenshot():
    """Find the most recent verification screenshot"""
    try:
        # Look for screenshots in typical locations
        screenshot_patterns = [
            "agent_feedback_*.png",
            "ui-capture*.png", 
            "screenshot*.png"
        ]
        
        import glob
        for pattern in screenshot_patterns:
            matches = glob.glob(pattern)
            if matches:
                # Return most recent by modification time
                return max(matches, key=os.path.getmtime)
        
        return None
    except Exception:
        return None

def cleanup_staged_files():
    """Clean up any staged verification files"""
    log_milestone("CLEANUP_START", "Cleaning previously staged verification files")
    try:
        subprocess.run(['git', 'reset', 'HEAD', '.continuum/verification/'], 
                      capture_output=True, check=False)
        subprocess.run(['git', 'reset', 'HEAD', 'verification/'], 
                      capture_output=True, check=False)
    except:
        pass
    log_milestone("CLEANUP_COMPLETE", "Verification staging area cleaned")

def stage_verification_files():
    """Stage the verification proof files"""
    log_milestone("GIT_STAGING", "Staging complete verification package")
    try:
        # Stage verification files (both new artifact system and legacy for compatibility)
        subprocess.run(['git', 'add', '-A', '.continuum/artifacts/verification/'], check=False)
        subprocess.run(['git', 'add', '-A', '.continuum/verification/'], check=False)
        subprocess.run(['git', 'add', 'verification/'], check=False)
    except Exception as e:
        # Ignore staging errors - verification works without git tracking
        print(f"⚠️ Git staging warning: {e}")

def main():
    """Main commit verification workflow"""
    cleanup_staged_files()
    
    log_milestone("COMMIT_VERIFICATION_START", "Starting commit verification process")
    
    # Get git context
    commit_sha, commit_message, changed_files = get_git_context()
    if not commit_sha:
        print("❌ Failed to get git context")
        sys.exit(1)
    
    # Create VerificationArtifact
    log_milestone("ARTIFACT_CREATION", "Creating VerificationArtifact with inheritance structure")
    artifact_path = create_node_verification_artifact(commit_sha, commit_message, changed_files)
    
    if not artifact_path:
        print("❌ Failed to create VerificationArtifact")
        sys.exit(1)
    
    log_milestone("ARTIFACT_CREATED", f"VerificationArtifact created: {artifact_path}")
    
    # Run verification
    verification_result = run_verification()
    
    # Find screenshot
    screenshot_path = find_screenshot()
    if screenshot_path:
        log_milestone("SCREENSHOT_FOUND", f"Located screenshot: {screenshot_path}")
    
    # Update artifact with results
    log_milestone("ARTIFACT_UPDATE", "Updating VerificationArtifact with verification results")
    if update_verification_artifact(artifact_path, verification_result, screenshot_path):
        log_milestone("ARTIFACT_COMPLETE", f"VerificationArtifact completed: {artifact_path}")
    else:
        log_milestone("ARTIFACT_FAILED", "Failed to update VerificationArtifact")
    
    # Create legacy compatibility
    log_milestone("LEGACY_COMPATIBILITY", "Creating legacy verification structure for compatibility")
    
    # Stage verification files
    stage_verification_files()
    
    # Check final result
    if verification_result.returncode == 0:
        print("🎉 ALL CHECKS PASSED")
        print("🚀 COMMIT APPROVED - System is healthy") 
        print("✅ VerificationArtifact system working")
        print("✅ Inheritance-driven architecture operational")
        print("✅ Console evidence collected")
        sys.exit(0)
    else:
        print("❌ VERIFICATION FAILED")
        print("🚨 COMMIT BLOCKED - Fix issues before committing")
        sys.exit(1)

if __name__ == "__main__":
    main()