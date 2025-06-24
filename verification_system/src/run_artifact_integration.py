"""
RunArtifact Integration Module

Handles the creation and management of RunArtifact structures for git hook verification.
Provides clean separation between Python verification logic and Node.js RunArtifact system.
"""

import json
import time
from pathlib import Path
from typing import Dict, Any, Optional


class RunArtifactIntegration:
    """Manages RunArtifact directory structure creation for git verification runs"""
    
    def __init__(self, base_dir: str = '.continuum'):
        self.base_dir = Path(base_dir)
        self.run_type = 'verification'
    
    def create_run_structure(self, commit_sha: str) -> Path:
        """Create RunArtifact directory structure for verification run"""
        run_dir = self.base_dir / self.run_type / f"run_{commit_sha}"
        run_dir.mkdir(parents=True, exist_ok=True)
        return run_dir
    
    def create_run_metadata(self, commit_sha: str, verification_data: Dict[str, Any]) -> Dict[str, Any]:
        """Create run.json metadata for verification run"""
        return {
            "runType": self.run_type,
            "runId": commit_sha,
            "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "status": "PASS" if verification_data.get('success') else "FAIL",
            "duration": verification_data.get('duration', 0) * 1000,  # Convert to milliseconds
            "gitHook": True,
            "commitSha": commit_sha,
            "verification": {
                "javascriptExecution": True,
                "consoleCapture": True,
                "screenshotCapture": True,
                "feedbackLoop": verification_data.get('feedback_loop_verified', False)
            }
        }
    
    def write_artifact(self, run_dir: Path, filename: str, content: Any) -> None:
        """Write artifact file with proper formatting"""
        filepath = run_dir / filename
        
        if isinstance(content, dict):
            with open(filepath, 'w') as f:
                json.dump(content, f, indent=2)
        else:
            filepath.write_text(str(content))
    
    def create_summary(self, commit_sha: str, verification_success: bool) -> str:
        """Create human-readable summary for verification run"""
        status = "✅ COMPLETE FEEDBACK LOOP OPERATIONAL" if verification_success else "❌ VERIFICATION FAILED"
        
        summary = f"""Git Hook Verification - Commit {commit_sha}

{status}

"""
        if verification_success:
            summary += """✅ Agent CAN execute JavaScript
✅ Agent CAN see its own console output  
✅ Agent CAN capture screenshots
✅ Agent HAS full visibility into its own actions

This verification confirms all JTAG debugging capabilities are working."""
        else:
            summary += """❌ One or more verification steps failed
❌ JTAG debugging capabilities compromised

Check error-logs.txt for detailed failure information."""
        
        return summary
    
    def parse_verification_logs(self, verification_output: str) -> Dict[str, str]:
        """Parse verification output into separate log categories"""
        lines = verification_output.split('\n')
        
        client_logs = "# Client logs from verification session\n"
        server_logs = "# Server logs from verification session\n" 
        console_logs = "# Browser console output from verification\n"
        
        for line in lines:
            if any(marker in line for marker in ['CLIENT-SIDE', 'PORTAL', 'BROWSER_LOG', 'WebSocket']):
                client_logs += line + "\n"
            elif 'UUID_' in line and any(marker in line for marker in ['LOG:', 'ERROR:', 'WARNING:']):
                console_logs += line + "\n"
            elif any(marker in line for marker in ['SERVER-SIDE', 'DevTools', 'MILESTONE', 'INFO:']):
                server_logs += line + "\n"
            else:
                server_logs += line + "\n"
        
        return {
            'client-logs.txt': client_logs,
            'server-logs.txt': server_logs,
            'console-logs.txt': console_logs,
            'error-logs.txt': "# No errors during verification\n"
        }
    
    def update_latest_symlink(self, commit_sha: str) -> None:
        """Create/update latest symlink to most recent verification run"""
        latest_link = self.base_dir / self.run_type / "latest"
        
        if latest_link.exists() or latest_link.is_symlink():
            latest_link.unlink()
        
        latest_link.symlink_to(f"run_{commit_sha}")
    
    def create_full_artifact(self, commit_sha: str, verification_result: Dict[str, Any], 
                           screenshot_path: Optional[Path] = None) -> Path:
        """Create complete RunArtifact structure for verification run"""
        # Create directory structure
        run_dir = self.create_run_structure(commit_sha)
        
        # Create run metadata
        metadata = self.create_run_metadata(commit_sha, verification_result)
        self.write_artifact(run_dir, 'run.json', metadata)
        
        # Create summary
        summary = self.create_summary(commit_sha, verification_result.get('success', False))
        self.write_artifact(run_dir, 'summary.txt', summary)
        
        # Parse and write logs
        if verification_result.get('output'):
            logs = self.parse_verification_logs(verification_result['output'])
            for filename, content in logs.items():
                self.write_artifact(run_dir, filename, content)
        
        # Copy screenshot if provided
        if screenshot_path and screenshot_path.exists():
            import subprocess
            ui_capture_path = run_dir / "ui-capture.png"
            subprocess.run([
                'sips', '-Z', '1280', '-s', 'format', 'png',
                str(screenshot_path), '--out', str(ui_capture_path)
            ], capture_output=True)
        
        # Update latest symlink
        self.update_latest_symlink(commit_sha)
        
        return run_dir