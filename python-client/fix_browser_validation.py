#!/usr/bin/env python3
"""
Fix Browser Validation - Use working Python client to debug and fix browser validation
"""

import asyncio
import websockets
import json
import base64
import time

async def fix_browser_validation():
    print("🔧 FIXING BROWSER VALIDATION - Python Debugger")
    print("=" * 60)
    print("Using working Python client to debug browser validation failure...\n")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Python client connected to Continuum")
            
            # Skip initial messages (like simple_js_test.py does)
            print("📥 Skipping initial WebSocket messages...")
            await websocket.recv()
            await websocket.recv()
            
            # Step 1: Test all browser validation milestones via Python
            print("\n🔍 STEP 1: Testing all browser milestones via Python client")
            
            milestones = {
                "connection": True,  # Already proven by connecting
                "jsExecution": False,
                "consoleCapture": False, 
                "errorSystems": False,
                "screenshot": False
            }
            
            # Test JavaScript Execution
            print("\n📝 Testing JavaScript Execution...")
            js_test = """
            console.log("🔧 Python debugger testing JS execution");
            return "JS_EXECUTION_SUCCESS";
            """
            
            encoded_js = base64.b64encode(js_test.encode()).decode()
            js_command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            await websocket.send(json.dumps(js_command))
            
            # Wait for proper bus command result (skip 'working' message)
            js_result = None
            for attempt in range(3):
                response = await websocket.recv()
                current_result = json.loads(response)
                
                if current_result.get('type') == 'result' and current_result.get('data', {}).get('role') == 'BusCommand':
                    js_result = current_result
                    break
                elif current_result.get('type') == 'working':
                    continue  # Skip working status, wait for actual result
            
            # Check JavaScript execution result
            if js_result:
                bus_result = js_result.get('data', {}).get('result', {}).get('result', {})
                browser_response = bus_result.get('browserResponse', {})
                if bus_result.get('executed') or browser_response.get('success'):
                    milestones["jsExecution"] = True
                    print("✅ JavaScript Execution: SUCCESS")
                else:
                    print("❌ JavaScript Execution: FAILED")
                    print(f"   Bus executed: {bus_result.get('executed')}, Browser success: {browser_response.get('success')}")
            else:
                print("❌ JavaScript Execution: FAILED")
                print(f"   Response: {js_result or 'No result received'}")
            
            # Test Console Capture  
            print("\n📋 Testing Console Capture...")
            console_test = """
            console.log("🔧 Python debugger: Console capture test");
            console.error("🔧 Python debugger: Test error");
            console.warn("🔧 Python debugger: Test warning");
            return "CONSOLE_CAPTURE_TEST";
            """
            
            encoded_console = base64.b64encode(console_test.encode()).decode()
            console_command = {
                'type': 'task', 
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_console}'
            }
            
            await websocket.send(json.dumps(console_command))
            
            # Wait for proper bus command result (skip 'working' message)
            console_result = None
            for attempt in range(3):
                response = await websocket.recv()
                current_result = json.loads(response)
                
                if current_result.get('type') == 'result' and current_result.get('data', {}).get('role') == 'BusCommand':
                    console_result = current_result
                    break
                elif current_result.get('type') == 'working':
                    continue  # Skip working status, wait for actual result
            
            # Check if console output was captured
            if console_result:
                bus_result = console_result.get('data', {}).get('result', {}).get('result', {})
                browser_response = bus_result.get('browserResponse', {})
                console_output = browser_response.get('output', [])
                if (bus_result.get('executed') or browser_response.get('success')) and len(console_output) > 0:
                    milestones["consoleCapture"] = True
                    print("✅ Console Capture: SUCCESS")
                    print(f"   Captured {len(console_output)} console messages")
                else:
                    print("❌ Console Capture: FAILED")
                    print(f"   Bus executed: {bus_result.get('executed')}, Browser success: {browser_response.get('success')}, Output count: {len(console_output)}")
            else:
                print("❌ Console Capture: FAILED")
                print("   No result received")
            
            # Test Error Systems
            print("\n🚨 Testing Error Systems...")
            error_test = """
            console.log("🔧 Python debugger: Error systems test");
            console.error("TEST_ERROR: Error detection test");
            console.warn("TEST_WARNING: Warning detection test");
            return "ERROR_SYSTEMS_VALIDATED";
            """
            
            encoded_error = base64.b64encode(error_test.encode()).decode()
            error_command = {
                'type': 'task',
                'role': 'system', 
                'task': f'[CMD:BROWSER_JS] {encoded_error}'
            }
            
            await websocket.send(json.dumps(error_command))
            
            # Wait for proper bus command result (skip 'working' message)
            error_result = None
            for attempt in range(3):
                response = await websocket.recv()
                current_result = json.loads(response)
                
                if current_result.get('type') == 'result' and current_result.get('data', {}).get('role') == 'BusCommand':
                    error_result = current_result
                    break
                elif current_result.get('type') == 'working':
                    continue  # Skip working status, wait for actual result
            
            # Check if error systems detected and captured errors
            if error_result:
                bus_result = error_result.get('data', {}).get('result', {}).get('result', {})
                browser_response = bus_result.get('browserResponse', {})
                console_output = browser_response.get('output', [])
                error_detected = any(msg.get('level') == 'error' for msg in console_output)
                warning_detected = any(msg.get('level') == 'warn' for msg in console_output)
                
                if (bus_result.get('executed') or browser_response.get('success')) and error_detected and warning_detected:
                    milestones["errorSystems"] = True
                    print("✅ Error Systems: SUCCESS")
                    print(f"   Detected errors and warnings in {len(console_output)} messages")
                else:
                    print("❌ Error Systems: FAILED")
                    print(f"   Bus executed: {bus_result.get('executed')}, Browser success: {browser_response.get('success')}, Errors: {error_detected}, Warnings: {warning_detected}")
            else:
                print("❌ Error Systems: FAILED")
                print("   No result received")
            
            # Test Screenshot (already proven working)
            print("\n📸 Testing Screenshot...")
            screenshot_command = {
                'type': 'task',
                'role': 'system',
                'task': '[CMD:SCREENSHOT] {"format": "png", "fullPage": true}'
            }
            
            await websocket.send(json.dumps(screenshot_command))
            print("📤 Screenshot command sent")
            
            # Wait for screenshot response
            screenshot_success = False
            start_time = time.time()
            
            while time.time() - start_time < 10:
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=1.0)
                    result = json.loads(response)
                    
                    if 'screenshot' in str(result).lower() or 'captured' in str(result).lower():
                        screenshot_success = True
                        break
                        
                except asyncio.TimeoutError:
                    continue
                except Exception as e:
                    break
            
            milestones["screenshot"] = screenshot_success
            print(f"{'✅' if screenshot_success else '❌'} Screenshot: {'SUCCESS' if screenshot_success else 'FAILED'}")
            
            # Step 2: Analyze results and provide diagnostics
            print(f"\n📊 MILESTONE ANALYSIS:")
            print(f"=" * 40)
            
            successful_milestones = sum(milestones.values())
            total_milestones = len(milestones)
            success_rate = (successful_milestones / total_milestones) * 100
            
            for milestone, success in milestones.items():
                status = "✅ PASS" if success else "❌ FAIL"
                print(f"  {milestone}: {status}")
            
            print(f"\n📈 Browser Validation Success Rate: {successful_milestones}/{total_milestones} ({success_rate:.1f}%)")
            
            # Step 3: Provide self-diagnostic instructions
            if successful_milestones == total_milestones:
                print(f"\n🎉 BROWSER VALIDATION: 100% SUCCESS!")
                print(f"All milestones working via Python client.")
                print(f"\n💡 DIAGNOSTIC RESULT:")
                print(f"  • Python client validation: ✅ Complete")
                print(f"  • JavaScript ClientConnection issue: Architecture problem")
                print(f"  • Solution: Use bus commands instead of separate connections")
                
            else:
                print(f"\n❌ BROWSER VALIDATION: {success_rate:.1f}% SUCCESS")
                print(f"\n💡 SELF-DIAGNOSTIC INSTRUCTIONS:")
                print(f"  1. Check server console for debug messages")
                print(f"  2. Verify WebSocket connection stability")
                print(f"  3. Test individual failing milestones")
                print(f"  4. Check command routing and processing")
                
                failed_milestones = [name for name, success in milestones.items() if not success]
                print(f"\n🔍 FAILED MILESTONES TO DEBUG:")
                for milestone in failed_milestones:
                    if milestone == "jsExecution":
                        print(f"  • {milestone}: Check BROWSER_JS command processing")
                    elif milestone == "consoleCapture":
                        print(f"  • {milestone}: Check console output capture in WebSocket response")
                    elif milestone == "errorSystems":
                        print(f"  • {milestone}: Check error/warning detection and categorization")
                    elif milestone == "screenshot":
                        print(f"  • {milestone}: Check SCREENSHOT command routing and html2canvas")
            
            # Step 4: Create validation completion report
            if successful_milestones == total_milestones:
                print(f"\n✅ VALIDATION FRAMEWORK COMPLETE!")
                print(f"Ready to:")
                print(f"  1. Check in working system")
                print(f"  2. Build widgets incrementally") 
                print(f"  3. Maintain 100% validation during development")
                
                return True
            else:
                print(f"\n🔧 VALIDATION FRAMEWORK NEEDS FIXES")
                print(f"Complete the failed milestones before building widgets.")
                
                return False
                
    except Exception as e:
        print(f"❌ Python debugger error: {e}")
        print(f"\n💡 SELF-DIAGNOSTIC INSTRUCTIONS:")
        print(f"  1. Check if Continuum server is running")
        print(f"  2. Verify WebSocket server is accepting connections")
        print(f"  3. Test basic connection with simple_js_test.py")
        print(f"  4. Restart server if needed: node continuum.cjs --daemon")
        return False

if __name__ == "__main__":
    result = asyncio.run(fix_browser_validation())
    print(f"\n🎯 FINAL DIAGNOSTIC RESULT: {'COMPLETE' if result else 'NEEDS_FIXES'}")