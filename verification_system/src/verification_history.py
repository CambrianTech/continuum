"""
Verification History Module

Manages the verification/history.txt file with proper formatting and legacy compatibility.
Handles both RunArtifact and legacy verification directory tracking.
"""

import time
import subprocess
from pathlib import Path
from typing import Dict, Any, Optional


class VerificationHistory:
    """Manages verification history tracking and reporting"""
    
    def __init__(self, history_path: str = 'verification/history.txt'):
        self.history_path = Path(history_path)
        self.ensure_history_file()
    
    def ensure_history_file(self) -> None:
        """Create history file with header if it doesn't exist"""
        if not self.history_path.exists():
            self.history_path.parent.mkdir(exist_ok=True)
            header = self.load_template('history_header.txt')
            self.history_path.write_text(header)
    
    def load_template(self, template_name: str) -> str:
        """Load text template from templates directory"""
        template_path = Path(__file__).parent.parent / 'templates' / template_name
        
        if template_path.exists():
            return template_path.read_text()
        
        # Fallback templates if file doesn't exist
        templates = {
            'history_header.txt': """# Continuum Verification History
# Format: DateTime | Status | Duration | SHA | CommitMessage | VerificationDir

""",
            'verification_summary.txt': """Git Hook Verification - Commit {commit_sha}

{status_line}

{details}
"""
        }
        
        return templates.get(template_name, f"# Template {template_name} not found\n")
    
    def get_commit_info(self) -> Dict[str, str]:
        """Get current commit information"""
        try:
            # Get commit message
            msg_result = subprocess.run(['git', 'log', '-1', '--pretty=format:%s'], 
                                      capture_output=True, text=True)
            commit_message = msg_result.stdout.strip() or "No commit message"
            
            # Get commit SHA
            sha_result = subprocess.run(['git', 'rev-parse', 'HEAD'], 
                                      capture_output=True, text=True)
            commit_sha = sha_result.stdout.strip()[:12]
            
            return {
                'message': commit_message,
                'sha': commit_sha
            }
        except:
            return {
                'message': "Unknown commit",
                'sha': "unknown"
            }
    
    def create_history_entry(self, status: str, duration: float, 
                           verification_dir: Optional[str] = None) -> str:
        """Create formatted history entry"""
        timestamp = time.strftime("%Y-%m-%d %H:%M:%S")
        commit_info = self.get_commit_info()
        
        verification_location = verification_dir if verification_dir else "none"
        
        return (f"{timestamp} | {status} | {duration:.1f}s | "
                f"{commit_info['sha']} | {commit_info['message']} | {verification_location}\n")
    
    def add_entry(self, status: str, duration: float, 
                  verification_dir: Optional[str] = None) -> None:
        """Add new entry to verification history"""
        entry = self.create_history_entry(status, duration, verification_dir)
        
        with open(self.history_path, 'a') as f:
            f.write(entry)
    
    def get_recent_entries(self, count: int = 10) -> list:
        """Get recent verification entries"""
        if not self.history_path.exists():
            return []
        
        lines = self.history_path.read_text().splitlines()
        # Filter out header and empty lines
        data_lines = [line for line in lines if line and not line.startswith('#')]
        
        return data_lines[-count:] if data_lines else []
    
    def get_stats(self) -> Dict[str, Any]:
        """Get verification statistics"""
        entries = self.get_recent_entries(50)  # Last 50 entries
        
        if not entries:
            return {'total': 0, 'pass_rate': 0, 'avg_duration': 0}
        
        passed = sum(1 for entry in entries if ' PASS ' in entry)
        total = len(entries)
        
        # Extract durations
        durations = []
        for entry in entries:
            parts = entry.split(' | ')
            if len(parts) > 2:
                try:
                    duration_str = parts[2].replace('s', '')
                    durations.append(float(duration_str))
                except ValueError:
                    pass
        
        avg_duration = sum(durations) / len(durations) if durations else 0
        
        return {
            'total': total,
            'passed': passed,
            'failed': total - passed,
            'pass_rate': (passed / total * 100) if total > 0 else 0,
            'avg_duration': avg_duration
        }