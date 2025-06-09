#!/usr/bin/env python3
"""
Test Screenshot Milestone - Use Python client to validate screenshot capability
"""

import asyncio
import websockets
import json
import base64
import time

async def test_screenshot_milestone():
    print("📸 SCREENSHOT MILESTONE TEST - Python Client")
    print("=" * 60)
    print("Testing screenshot capability via Python client...\n")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to Continuum WebSocket server")
            
            # Test 1: Check if browser has screenshot prerequisites
            print("\n🔍 Step 1: Check screenshot prerequisites in browser")
            
            prereq_js = """
            console.log("📸 SCREENSHOT MILESTONE: Checking prerequisites");
            const result = {
                html2canvas: typeof html2canvas !== 'undefined',
                websocket: typeof WebSocket !== 'undefined', 
                wsConnection: window.ws ? window.ws.readyState === WebSocket.OPEN : false,
                documentReady: document.readyState === 'complete',
                canvasSupport: !!document.createElement('canvas').getContext
            };
            console.log("📊 Prerequisites:", result);
            JSON.stringify(result);
            """
            
            encoded_js = base64.b64encode(prereq_js.encode()).decode()
            
            prereq_command = {
                'type': 'task',
                'role': 'system', 
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            await websocket.send(json.dumps(prereq_command))
            print("📤 Sent prerequisite check command")
            
            # Wait for response
            response = await websocket.recv()
            result = json.loads(response)
            print(f"📥 Response type: {result.get('type')}")
            
            # Test 2: Try direct screenshot command
            print("\n🔍 Step 2: Send direct screenshot command")
            
            screenshot_command = {
                'type': 'task',
                'role': 'system',
                'task': '[CMD:SCREENSHOT] {"format": "png", "fullPage": true}'
            }
            
            await websocket.send(json.dumps(screenshot_command))
            print("📤 Sent screenshot command")
            
            # Wait for screenshot response with timeout
            print("⏰ Waiting for screenshot response (15 second timeout)...")
            
            screenshot_success = False
            start_time = time.time()
            timeout = 15
            
            while time.time() - start_time < timeout:
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=1.0)
                    result = json.loads(response)
                    
                    print(f"📥 Response: {result.get('type')} - {str(result)[:100]}...")
                    
                    # Look for screenshot success indicators
                    if 'screenshot' in str(result).lower() or 'captured' in str(result).lower():
                        print("✅ Screenshot response detected!")
                        screenshot_success = True
                        break
                        
                except asyncio.TimeoutError:
                    continue
                except Exception as e:
                    print(f"❌ Error receiving response: {e}")
                    break
            
            # Test 3: Check browser console for debug messages
            print("\n🔍 Step 3: Check browser console for debug messages")
            
            console_check_js = """
            console.log("📸 SCREENSHOT MILESTONE: Checking for debug messages");
            console.log("Looking for screenshot-related console messages...");
            return "CONSOLE_CHECK_COMPLETE";
            """
            
            encoded_console = base64.b64encode(console_check_js.encode()).decode()
            
            console_command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_console}'
            }
            
            await websocket.send(json.dumps(console_command))
            
            # Get console response
            console_response = await websocket.recv()
            console_result = json.loads(console_response)
            print(f"📥 Console check response: {console_result.get('type')}")
            
            # Summary
            print(f"\n📊 SCREENSHOT MILESTONE RESULTS:")
            print(f"Screenshot Command Sent: ✅")
            print(f"Screenshot Response Received: {'✅' if screenshot_success else '❌'}")
            print(f"Connection Stable: ✅")
            
            if not screenshot_success:
                print("\n❌ SCREENSHOT MILESTONE: FAILED")
                print("📋 Possible issues:")
                print("  1. Screenshot command not reaching command processor")
                print("  2. Command processor not executing screenshot properly")
                print("  3. WebSocket response not being sent back")
                print("  4. Response format not recognized")
                print("\n💡 Check server console for debug messages from:")
                print("  - CommandProcessor.routeToScreenshotCommand")
                print("  - ScreenshotCommand.execute")
            else:
                print("\n✅ SCREENSHOT MILESTONE: SUCCESS")
            
            return screenshot_success
            
    except Exception as e:
        print(f"❌ Connection error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(test_screenshot_milestone())
    print(f"\n🎯 Final Result: {'SUCCESS' if result else 'FAILED'}")