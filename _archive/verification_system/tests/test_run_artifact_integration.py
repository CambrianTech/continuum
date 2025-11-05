"""
Unit tests for RunArtifact Integration Module
"""

import unittest
import tempfile
import json
import sys
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

from src.run_artifact_integration import RunArtifactIntegration


class TestRunArtifactIntegration(unittest.TestCase):
    
    def setUp(self):
        self.temp_dir = tempfile.mkdtemp()
        self.integration = RunArtifactIntegration(self.temp_dir)
        self.test_sha = "abc123def456"
    
    def test_create_run_structure(self):
        """Test creation of RunArtifact directory structure"""
        run_dir = self.integration.create_run_structure(self.test_sha)
        
        expected_path = Path(self.temp_dir) / 'verification' / f'run_{self.test_sha}'
        self.assertEqual(run_dir, expected_path)
        self.assertTrue(run_dir.exists())
        self.assertTrue(run_dir.is_dir())
    
    def test_create_run_metadata(self):
        """Test creation of run.json metadata"""
        verification_data = {
            'success': True,
            'duration': 25.5,
            'feedback_loop_verified': True
        }
        
        metadata = self.integration.create_run_metadata(self.test_sha, verification_data)
        
        self.assertEqual(metadata['runType'], 'verification')
        self.assertEqual(metadata['runId'], self.test_sha)
        self.assertEqual(metadata['status'], 'PASS')
        self.assertEqual(metadata['duration'], 25500)  # Should be in milliseconds
        self.assertTrue(metadata['gitHook'])
        self.assertEqual(metadata['commitSha'], self.test_sha)
        self.assertTrue(metadata['verification']['feedbackLoop'])
    
    def test_write_artifact_json(self):
        """Test writing JSON artifacts"""
        run_dir = self.integration.create_run_structure(self.test_sha)
        test_data = {"test": "data", "number": 42}
        
        self.integration.write_artifact(run_dir, 'test.json', test_data)
        
        artifact_path = run_dir / 'test.json'
        self.assertTrue(artifact_path.exists())
        
        with open(artifact_path) as f:
            loaded_data = json.load(f)
        self.assertEqual(loaded_data, test_data)
    
    def test_write_artifact_text(self):
        """Test writing text artifacts"""
        run_dir = self.integration.create_run_structure(self.test_sha)
        test_content = "This is test content\nWith multiple lines"
        
        self.integration.write_artifact(run_dir, 'test.txt', test_content)
        
        artifact_path = run_dir / 'test.txt'
        self.assertTrue(artifact_path.exists())
        self.assertEqual(artifact_path.read_text(), test_content)
    
    def test_create_summary_success(self):
        """Test creation of successful verification summary"""
        summary = self.integration.create_summary(self.test_sha, True)
        
        self.assertIn(self.test_sha, summary)
        self.assertIn("✅ COMPLETE FEEDBACK LOOP OPERATIONAL", summary)
        self.assertIn("Agent CAN execute JavaScript", summary)
        self.assertIn("JTAG debugging capabilities", summary)
    
    def test_create_summary_failure(self):
        """Test creation of failed verification summary"""
        summary = self.integration.create_summary(self.test_sha, False)
        
        self.assertIn(self.test_sha, summary)
        self.assertIn("❌ VERIFICATION FAILED", summary)
        self.assertIn("capabilities compromised", summary)
    
    def test_parse_verification_logs(self):
        """Test parsing of verification output into log categories"""
        test_output = """[INFO] SERVER-SIDE: Starting verification
🎯 MILESTONE: Browser launch
CLIENT-SIDE: Portal connection established
🌐 [timestamp] LOG: UUID_abc123 Console message from browser
🌐 [timestamp] ERROR: UUID_abc123 Browser error message
PORTAL: WebSocket connected
DevTools: Screenshot captured"""
        
        logs = self.integration.parse_verification_logs(test_output)
        
        self.assertIn('client-logs.txt', logs)
        self.assertIn('server-logs.txt', logs)
        self.assertIn('console-logs.txt', logs)
        
        # Debug: Print what's actually in console logs
        # print(f"Console logs content: {repr(logs['console-logs.txt'])}")
        
        # Check that logs are properly categorized
        self.assertIn('CLIENT-SIDE', logs['client-logs.txt'])
        self.assertIn('PORTAL', logs['client-logs.txt'])
        self.assertIn('SERVER-SIDE', logs['server-logs.txt'])
        self.assertIn('MILESTONE', logs['server-logs.txt'])
        
        # Fix test to match actual parsing logic
        self.assertIn('UUID_abc123 Console message', logs['console-logs.txt'])
        self.assertIn('UUID_abc123 Browser error', logs['console-logs.txt'])
    
    def test_update_latest_symlink(self):
        """Test creation and update of latest symlink"""
        # Create run structure first
        run_dir = self.integration.create_run_structure(self.test_sha)
        
        # Create symlink
        self.integration.update_latest_symlink(self.test_sha)
        
        latest_link = Path(self.temp_dir) / 'verification' / 'latest'
        self.assertTrue(latest_link.is_symlink())
        self.assertEqual(latest_link.readlink(), Path(f'run_{self.test_sha}'))
        
        # Test updating symlink
        new_sha = "def456ghi789"
        new_run_dir = self.integration.create_run_structure(new_sha)
        self.integration.update_latest_symlink(new_sha)
        
        self.assertEqual(latest_link.readlink(), Path(f'run_{new_sha}'))


if __name__ == '__main__':
    unittest.main()