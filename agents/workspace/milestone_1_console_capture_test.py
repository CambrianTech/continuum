#!/usr/bin/env python3
"""
MILESTONE 1: Console Output Capture Test
======================================

Debug exactly what's happening with console capture mechanism.
Test the actual JavaScript execution and console interception.
"""

import asyncio
import websockets
import json
import base64
import time
from datetime import datetime

class ConsoleCaptureTester:
    def __init__(self, ws_url="ws://localhost:9000"):
        self.ws_url = ws_url
        self.console_output = []
        
    async def test_console_capture_mechanism(self):
        """Test the console capture mechanism directly"""
        print("\n🔍 MILESTONE 1: Testing Console Capture Mechanism")
        print("=" * 60)
        
        try:
            async with websockets.connect(self.ws_url) as websocket:
                
                # Skip connection banner
                try:
                    banner = await asyncio.wait_for(websocket.recv(), timeout=3.0)
                    print(f"📡 Connection established")
                except:
                    pass
                
                # Test simple console output capture
                console_test_js = '''
                console.log("🧪 CONSOLE TEST: Starting console capture test");
                console.error("TEST_ERROR: This is a test error for capture");
                console.warn("TEST_WARNING: This is a test warning for capture");
                console.log("✅ CONSOLE TEST: All test messages sent");
                
                // Return test completion signal
                "CONSOLE_CAPTURE_TEST_COMPLETE";
                '''
                
                # Send as task (the working method from version detection)
                encoded_js = base64.b64encode(console_test_js.encode()).decode()
                task_message = {
                    'type': 'task',
                    'role': 'system',
                    'task': f'[CMD:BROWSER_JS] {encoded_js}'
                }
                
                print("📤 Sending console capture test...")
                await websocket.send(json.dumps(task_message))
                
                # Collect ALL responses for detailed analysis
                responses = []
                for attempt in range(5):
                    try:
                        response = await asyncio.wait_for(websocket.recv(), timeout=8.0)
                        result = json.loads(response)
                        responses.append(result)
                        print(f"📥 Response {attempt+1}: {result.get('type')} - {result.get('message', '')[:100]}...")
                        
                        # Look for js_executed response with console output
                        if result.get('type') == 'js_executed':
                            print(f"🎯 FOUND js_executed response!")
                            print(f"   Success: {result.get('success')}")
                            print(f"   Output: {result.get('output', [])}")
                            print(f"   Result: {result.get('result')}")
                            
                            self.console_output = result.get('output', [])
                            
                            if len(self.console_output) > 0:
                                print("✅ MILESTONE 1 SUCCESS: Console output captured!")
                                self.analyze_console_output()
                                return True
                            else:
                                print("❌ Console output array is empty")
                        
                        # Also check if result contains the completion signal
                        if 'CONSOLE_CAPTURE_TEST_COMPLETE' in str(result):
                            print("✅ JavaScript execution confirmed (completion signal found)")
                            
                    except asyncio.TimeoutError:
                        print(f"⏱️ Timeout on response {attempt+1}")
                        break
                
                print("\n📋 ALL RESPONSES ANALYSIS:")
                for i, resp in enumerate(responses):
                    print(f"  {i+1}. Type: {resp.get('type')}, Keys: {list(resp.keys())}")
                
                # If we got here, console capture didn't work as expected
                print("❌ Console capture mechanism not working as expected")
                return False
                
        except Exception as e:
            print(f"❌ Console capture test failed: {e}")
            return False
    
    def analyze_console_output(self):
        """Analyze captured console output"""
        print("\n📊 CONSOLE OUTPUT ANALYSIS:")
        print("-" * 40)
        
        error_count = 0
        warning_count = 0
        log_count = 0
        
        for entry in self.console_output:
            if isinstance(entry, dict):
                level = entry.get('level', 'unknown')
                message = entry.get('message', 'no message')
                
                if level == 'error':
                    error_count += 1
                    print(f"🚨 ERROR: {message}")
                elif level == 'warn':
                    warning_count += 1
                    print(f"⚠️  WARNING: {message}")
                elif level == 'log':
                    log_count += 1
                    print(f"📝 LOG: {message}")
                else:
                    print(f"❓ {level.upper()}: {message}")
            else:
                print(f"🔍 Raw entry: {entry}")
        
        print(f"\n📈 SUMMARY:")
        print(f"   Total entries: {len(self.console_output)}")
        print(f"   Errors: {error_count}")
        print(f"   Warnings: {warning_count}")
        print(f"   Logs: {log_count}")
        
        # Validate we captured the test messages
        test_error_found = any('TEST_ERROR' in str(entry) for entry in self.console_output)
        test_warning_found = any('TEST_WARNING' in str(entry) for entry in self.console_output)
        console_test_found = any('CONSOLE TEST' in str(entry) for entry in self.console_output)
        
        print(f"\n🎯 TEST MESSAGE VALIDATION:")
        print(f"   Test error captured: {'✅' if test_error_found else '❌'}")
        print(f"   Test warning captured: {'✅' if test_warning_found else '❌'}")
        print(f"   Console test messages: {'✅' if console_test_found else '❌'}")
        
        if test_error_found and test_warning_found and console_test_found:
            print("🎉 MILESTONE 1 FULLY VALIDATED: All console capture working!")
            return True
        else:
            print("⚠️ Some test messages missing from capture")
            return False

async def main():
    """Run console capture test for MILESTONE 1"""
    print("🔥 MILESTONE 1: CONSOLE CAPTURE DEBUG TEST")
    print("=" * 60)
    print("Testing the exact console capture mechanism...")
    
    tester = ConsoleCaptureTester()
    success = await tester.test_console_capture_mechanism()
    
    if success:
        print("\n🎯 MILESTONE 1: CONSOLE CAPTURE WORKING!")
        print("✅ Ready to proceed to MILESTONE 3")
    else:
        print("\n🔧 MILESTONE 1: NEEDS DEBUGGING")
        print("❌ Console capture mechanism requires investigation")
        
    return success

if __name__ == "__main__":
    asyncio.run(main())