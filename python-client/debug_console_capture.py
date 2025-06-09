#!/usr/bin/env python3
"""
Debug Console Capture - Test JavaScript execution and console output capture
"""

import asyncio
import websockets
import json
import base64

async def debug_console_capture():
    print("🔍 DEBUG CONSOLE CAPTURE - Detailed Analysis")
    print("=" * 60)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to Continuum WebSocket server")
            
            # Skip initial messages
            print("📥 Skipping initial WebSocket messages...")
            await websocket.recv()
            await websocket.recv()
            
            # Test 1: Simple console output capture
            print("\n🔍 TEST 1: Simple console.log capture")
            
            js_test = """
            console.log("🔧 DEBUG: Test message 1");
            console.log("🔧 DEBUG: Test message 2"); 
            console.error("🔧 DEBUG: Test error");
            console.warn("🔧 DEBUG: Test warning");
            return "CONSOLE_TEST_SUCCESS";
            """
            
            encoded_js = base64.b64encode(js_test.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print(f"📤 Sending command: {command['task'][:50]}...")
            await websocket.send(json.dumps(command))
            
            # Wait for responses (first is usually 'working', then comes 'result')
            print("⏰ Waiting for response...")
            
            result = None
            attempts = 0
            max_attempts = 3
            
            while attempts < max_attempts:
                response = await websocket.recv()
                current_result = json.loads(response)
                
                print(f"📥 Response {attempts + 1}: {current_result.get('type')}")
                
                if current_result.get('type') == 'result':
                    result = current_result
                    break
                elif current_result.get('type') == 'working':
                    print("   (Working status - waiting for actual result...)")
                    attempts += 1
                    continue
                else:
                    result = current_result
                    break
            
            if not result:
                print("❌ No result received after waiting for multiple responses")
                return False
            
            print(f"\n📥 DETAILED RESPONSE ANALYSIS:")
            print(f"Response Type: {result.get('type')}")
            print(f"Response Keys: {list(result.keys())}")
            
            if 'data' in result:
                data = result['data']
                if isinstance(data, dict):
                    print(f"\nData Keys: {list(data.keys())}")
                    print(f"Data: {json.dumps(data, indent=2)}")
                else:
                    print(f"\nData (string): {data}")
            
            # Check if it contains the execution results
            if result.get('type') == 'result':
                data = result.get('data', {})
                if 'browserResponse' in data:
                    browser_response = data['browserResponse']
                    print(f"\n🌐 BROWSER RESPONSE FOUND:")
                    print(f"Success: {browser_response.get('success')}")
                    print(f"Output: {browser_response.get('output', [])}")
                    print(f"Result: {browser_response.get('result')}")
                    print(f"Error: {browser_response.get('error')}")
                else:
                    print(f"\n❌ NO BROWSER RESPONSE - Only data keys: {list(data.keys())}")
                    
            print(f"\n📊 DIAGNOSIS:")
            if result.get('type') == 'result':
                print("✅ Command processed by server")
                data = result.get('data', {})
                if 'browserResponse' in data:
                    br = data['browserResponse']
                    if br.get('success') and br.get('output'):
                        print("✅ Browser executed JavaScript successfully")
                        print("✅ Console output captured")
                        print("✅ CONSOLE CAPTURE: WORKING!")
                        return True
                    else:
                        print("❌ Browser response missing output")
                else:
                    print("❌ Browser response not included in server response")
            else:
                print(f"❌ Unexpected response type: {result.get('type')}")
                
            print("❌ CONSOLE CAPTURE: FAILED")
            return False
            
    except Exception as e:
        print(f"❌ Error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(debug_console_capture())
    print(f"\n🎯 Final Result: {'SUCCESS' if result else 'FAILED'}")