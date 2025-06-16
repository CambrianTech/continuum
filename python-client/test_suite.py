#!/usr/bin/env python3
"""
Clean, Simple Test Suite for Continuum
Tests the core functionality that actually works
"""

import asyncio
import sys
import os
import json
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class ContinuumTestSuite:
    """Clean test suite for core functionality"""
    
    def __init__(self):
        self.base_dir = Path(__file__).parent.parent
        self.screenshots_dir = self.base_dir / '.continuum' / 'screenshots'
        self.results = []
        
    def log(self, message):
        print(f"🧪 {message}")
        
    def add_result(self, test_name, passed, details=""):
        self.results.append({
            'test': test_name,
            'passed': passed,
            'details': details
        })
        status = "✅" if passed else "❌"
        self.log(f"{status} {test_name}: {details}")
        
    async def test_daemon_connection(self):
        """Test WebSocket connection to daemon"""
        try:
            load_continuum_config()
            async with ContinuumClient() as client:
                await client.register_agent({
                    'agentId': 'test-daemon',
                    'agentName': 'Daemon Test',
                    'agentType': 'ai'
                })
                self.add_result("Daemon Connection", True, "Connected successfully")
                return True
        except Exception as e:
            self.add_result("Daemon Connection", False, str(e))
            return False
            
    async def test_proper_screenshot_api(self):
        """Test the working screenshot API (command.screenshot)"""
        try:
            async with ContinuumClient() as client:
                await client.register_agent({
                    'agentId': 'test-screenshot',
                    'agentName': 'Screenshot Test',
                    'agentType': 'ai'
                })
                
                # Use the WORKING screenshot API 
                result = await client.command.screenshot()
                
                if result:
                    self.add_result("Screenshot API", True, "Screenshot command executed")
                    return True
                else:
                    self.add_result("Screenshot API", False, "Screenshot command failed")
                    return False
                    
        except Exception as e:
            self.add_result("Screenshot API", False, str(e))
            return False
    
    async def test_basic_js_execution(self):
        """Test basic JavaScript execution"""
        try:
            async with ContinuumClient() as client:
                await client.register_agent({
                    'agentId': 'test-js',
                    'agentName': 'JS Test',
                    'agentType': 'ai'
                })
                
                # Simple JS test that should work
                result = await client.js.execute("""
                    console.log('Test execution');
                    return 'SUCCESS';
                """)
                
                if result.get('success') and result.get('result') == 'SUCCESS':
                    self.add_result("JavaScript Execution", True, "JS executed successfully")
                    return True
                else:
                    self.add_result("JavaScript Execution", False, f"JS failed: {result}")
                    return False
                    
        except Exception as e:
            self.add_result("JavaScript Execution", False, str(e))
            return False
    
    async def run_tests(self):
        """Run all tests"""
        self.log("Starting Clean Test Suite...")
        self.log("=" * 50)
        
        # Run core tests
        await self.test_daemon_connection()
        await self.test_basic_js_execution()
        await self.test_proper_screenshot_api()
        
        # Summary
        self.log("\n" + "=" * 50)
        self.log("TEST RESULTS:")
        
        passed_count = sum(1 for r in self.results if r['passed'])
        total_count = len(self.results)
        
        for result in self.results:
            status = "✅" if result['passed'] else "❌"
            self.log(f"  {status} {result['test']}: {result['details']}")
        
        self.log(f"\nFINAL: {passed_count}/{total_count} tests passed")
        
        if passed_count == total_count:
            self.log("🎉 ALL TESTS PASSED")
            return True
        else:
            self.log("⚠️ SOME TESTS FAILED")
            return False

async def main():
    """Run test suite"""
    suite = ContinuumTestSuite()
    success = await suite.run_tests()
    sys.exit(0 if success else 1)

if __name__ == "__main__":
    asyncio.run(main())