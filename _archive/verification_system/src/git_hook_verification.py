"""
Git Hook Verification Module

Core verification logic that orchestrates the git hook verification process.
Handles subprocess execution, result validation, and error handling.
"""

import subprocess
import sys
import time
from pathlib import Path
from typing import Dict, Any, Optional, Tuple


class GitHookVerification:
    """Main verification controller for git hook commits"""
    
    def __init__(self, verification_script: str = 'devtools_full_demo.py'):
        self.verification_script = verification_script
        self.start_time = None
        self.commit_sha = None
    
    def get_commit_sha(self) -> str:
        """Get current commit SHA for run identification"""
        try:
            result = subprocess.run(['git', 'rev-parse', 'HEAD'], 
                                  capture_output=True, text=True)
            return result.stdout.strip()[:12]  # First 12 chars
        except:
            return time.strftime("%Y%m%d_%H%M%S")  # Fallback to timestamp
    
    def should_skip_verification(self) -> bool:
        """Check if this commit should skip verification (e.g., verification commits)"""
        try:
            commit_msg = Path('.git/COMMIT_EDITMSG').read_text()
            skip_words = ['verification', 'screenshot', 'test commit']
            return any(word in commit_msg.lower() for word in skip_words)
        except:
            return False
    
    def run_verification_subprocess(self) -> subprocess.CompletedProcess:
        """Execute the verification subprocess with timeout"""
        return subprocess.run([
            sys.executable, self.verification_script, '--commit-check'
        ], capture_output=True, text=True, timeout=60)
    
    def validate_verification_result(self, result: subprocess.CompletedProcess) -> Dict[str, Any]:
        """Validate verification result and extract key indicators"""
        success_indicators = [
            'BIDIRECTIONAL FEEDBACK VERIFIED',
            'COMPLETE FEEDBACK LOOP OPERATIONAL',
            'Agent CAN execute JavaScript',
            'Agent CAN see its own console output',
            'Agent CAN capture screenshots'
        ]
        
        if result.returncode != 0:
            return {
                'success': False,
                'output': result.stdout,
                'error': f"Process failed with exit code {result.returncode}",
                'feedback_loop_verified': False
            }
        
        # Check for all success indicators in output
        output = result.stdout
        indicators_found = [indicator for indicator in success_indicators 
                          if indicator in output]
        
        success = len(indicators_found) == len(success_indicators)
        
        return {
            'success': success,
            'output': output,
            'indicators_found': indicators_found,
            'feedback_loop_verified': success,
            'duration': time.time() - self.start_time if self.start_time else 0
        }
    
    def find_verification_screenshot(self) -> Optional[Path]:
        """Find the most recent verification screenshot"""
        screenshots_dir = Path('.continuum/screenshots/')
        if not screenshots_dir.exists():
            return None
        
        screenshots = list(screenshots_dir.glob('agent_feedback_*.png'))
        if not screenshots:
            return None
        
        # Return most recent screenshot
        return max(screenshots, key=lambda p: p.stat().st_mtime)
    
    def run_full_verification(self) -> Tuple[bool, Dict[str, Any], Optional[Path]]:
        """Run complete verification process and return results"""
        self.start_time = time.time()
        self.commit_sha = self.get_commit_sha()
        
        # Skip verification for certain commit types
        if self.should_skip_verification():
            return True, {'success': True, 'skipped': True}, None
        
        try:
            # Execute verification subprocess
            result = self.run_verification_subprocess()
            
            # Validate results
            verification_data = self.validate_verification_result(result)
            
            # Find screenshot
            screenshot_path = self.find_verification_screenshot()
            
            return verification_data['success'], verification_data, screenshot_path
            
        except subprocess.TimeoutExpired:
            return False, {
                'success': False,
                'error': 'Verification timeout after 60 seconds',
                'duration': 60
            }, None
        except Exception as e:
            return False, {
                'success': False,
                'error': f"Verification failed: {str(e)}",
                'duration': time.time() - self.start_time if self.start_time else 0
            }, None