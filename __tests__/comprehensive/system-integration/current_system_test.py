#!/usr/bin/env python3
"""
Current System Test - Tests what's actually working RIGHT NOW
Created: $(date)
Tests the current state of the system as it exists today
"""

import asyncio
import sys
import os
import time
from pathlib import Path
from datetime import datetime

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class CurrentSystemTest:
    """Test the system as it exists right now"""
    
    def __init__(self):
        self.base_dir = Path(__file__).parent.parent
        self.screenshots_dir = self.base_dir / '.continuum' / 'screenshots'
        self.results = []
        self.start_time = datetime.now()
        
    def log(self, message):
        timestamp = datetime.now().strftime('%H:%M:%S')
        print(f"[{timestamp}] {message}")
        
    def add_result(self, test_name, passed, details=""):
        self.results.append({
            'test': test_name,
            'passed': passed,
            'details': details,
            'timestamp': datetime.now()
        })
        status = "✅" if passed else "❌"
        self.log(f"{status} {test_name}: {details}")
        
    async def test_current_daemon_status(self):
        """Test 1: Check if daemon is currently running and responsive"""
        try:
            load_continuum_config()
            async with ContinuumClient() as client:
                await client.register_agent({
                    'agentId': f'current-test-{int(time.time())}',
                    'agentName': 'Current System Test',
                    'agentType': 'ai'
                })
                self.add_result("Current Daemon Status", True, "Daemon responsive")
                return client
        except Exception as e:
            self.add_result("Current Daemon Status", False, f"Daemon error: {e}")
            return None
            
    async def test_screenshot_files_exist(self, client):
        """Test 2: Check if screenshot directory and recent files exist"""
        try:
            # Check directory exists
            if not self.screenshots_dir.exists():
                self.add_result("Screenshot Files", False, "Screenshot directory doesn't exist")
                return False
                
            # Get all PNG files
            png_files = list(self.screenshots_dir.glob('*.png'))
            if not png_files:
                self.add_result("Screenshot Files", False, "No PNG files found")
                return False
                
            # Check for recent files (within last hour)
            now = time.time()
            recent_files = []
            for f in png_files:
                file_age = now - f.stat().st_mtime
                if file_age < 3600:  # Less than 1 hour old
                    recent_files.append(f)
                    
            if recent_files:
                newest = max(recent_files, key=lambda f: f.stat().st_mtime)
                file_size = newest.stat().st_size
                self.add_result("Screenshot Files", True, f"Found {len(recent_files)} recent files, newest: {newest.name} ({file_size} bytes)")
                return True
            else:
                self.add_result("Screenshot Files", False, f"No recent files (found {len(png_files)} total)")
                return False
                
        except Exception as e:
            self.add_result("Screenshot Files", False, f"File check error: {e}")
            return False
    
    async def test_create_new_screenshot(self, client):
        """Test 3: Actually create a new screenshot and verify it"""
        try:
            # Get current file count
            before_files = list(self.screenshots_dir.glob('*.png')) if self.screenshots_dir.exists() else []
            before_count = len(before_files)
            
            # Create screenshot using working API
            timestamp_before = time.time()
            result = await client.command.screenshot()
            
            if not result:
                self.add_result("Create Screenshot", False, "Screenshot command returned False")
                return False
                
            # Wait for file creation
            time.sleep(2)
            
            # Check for new files
            after_files = list(self.screenshots_dir.glob('*.png')) if self.screenshots_dir.exists() else []
            new_files = []
            
            for f in after_files:
                if f.stat().st_mtime > timestamp_before:
                    new_files.append(f)
                    
            if new_files:
                newest = max(new_files, key=lambda f: f.stat().st_mtime)
                file_size = newest.stat().st_size
                
                # Verify it's a real file (not corrupted 1-byte)
                if file_size > 100:
                    self.add_result("Create Screenshot", True, f"Created {newest.name} ({file_size} bytes)")
                    return True
                else:
                    self.add_result("Create Screenshot", False, f"File too small: {file_size} bytes")
                    return False
            else:
                self.add_result("Create Screenshot", False, "No new files created")
                return False
                
        except Exception as e:
            self.add_result("Create Screenshot", False, f"Screenshot creation error: {e}")
            return False
    
    async def test_basic_js_execution(self, client):
        """Test 4: Execute JavaScript and get a real result"""
        try:
            test_code = f"""
                console.log('Current system test at {datetime.now()}');
                return {{
                    timestamp: Date.now(),
                    location: window.location.href,
                    userAgent: navigator.userAgent.substring(0, 50)
                }};
            """
            
            result = await client.js.execute(test_code)
            
            if result.get('success'):
                # Try to parse the result
                try:
                    data = eval(result.get('result', '{}'))
                    if data.get('timestamp'):
                        self.add_result("JavaScript Execution", True, f"JS executed, timestamp: {data['timestamp']}")
                        return True
                    else:
                        self.add_result("JavaScript Execution", False, "No timestamp in result")
                        return False
                except:
                    self.add_result("JavaScript Execution", False, "Could not parse JS result")
                    return False
            else:
                self.add_result("JavaScript Execution", False, f"JS failed: {result}")
                return False
                
        except Exception as e:
            self.add_result("JavaScript Execution", False, f"JS execution error: {e}")
            return False
    
    async def run_current_tests(self):
        """Run tests on current system state"""
        self.log(f"🔍 CURRENT SYSTEM TEST - {self.start_time.strftime('%Y-%m-%d %H:%M:%S')}")
        self.log("=" * 60)
        self.log("Testing the system AS IT EXISTS RIGHT NOW")
        self.log("=" * 60)
        
        # Test 1: Basic connectivity
        client = await self.test_current_daemon_status()
        if not client:
            self.log("❌ CRITICAL: Cannot connect to current system")
            return False
        
        try:
            # Test current functionality
            await self.test_screenshot_files_exist(client)
            await self.test_basic_js_execution(client)
            await self.test_create_new_screenshot(client)
            
        except Exception as e:
            self.add_result("System Test", False, f"Test error: {e}")
        
        # Results
        self.log("\n" + "=" * 60)
        self.log("🎯 CURRENT SYSTEM TEST RESULTS:")
        self.log("=" * 60)
        
        passed_count = sum(1 for r in self.results if r['passed'])
        total_count = len(self.results)
        
        for result in self.results:
            status = "✅" if result['passed'] else "❌"
            timestamp = result['timestamp'].strftime('%H:%M:%S')
            self.log(f"[{timestamp}] {status} {result['test']}: {result['details']}")
        
        self.log("\n" + "=" * 60)
        self.log(f"🏁 FINAL: {passed_count}/{total_count} current tests passed")
        
        if passed_count == total_count:
            self.log("🎉 CURRENT SYSTEM IS FULLY OPERATIONAL")
            return True
        else:
            self.log("⚠️ CURRENT SYSTEM HAS ISSUES")
            return False

async def main():
    """Run current system test"""
    test = CurrentSystemTest()
    success = await test.run_current_tests()
    sys.exit(0 if success else 1)

if __name__ == "__main__":
    asyncio.run(main())