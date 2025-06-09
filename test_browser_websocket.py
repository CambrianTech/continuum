#!/usr/bin/env python3
"""
Test browser WebSocket connection specifically for screenshot_data sending
"""
import asyncio
import websockets
import json
import base64

async def test_browser_websocket():
    print("🔍 TESTING BROWSER WEBSOCKET CONNECTION")
    print("=" * 50)
    print("Use working bus commands to test browser's window.ws connection")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Debugger connected")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Test browser WebSocket connection status
            ws_test_js = '''
            console.log("🔍 BROWSER WS: Testing window.ws connection");
            console.log("🔍 BROWSER WS: window.ws exists:", typeof window.ws !== 'undefined');
            
            if (window.ws) {
                console.log("🔍 BROWSER WS: readyState:", window.ws.readyState);
                console.log("🔍 BROWSER WS: WebSocket.OPEN =", WebSocket.OPEN);
                console.log("🔍 BROWSER WS: Is connected:", window.ws.readyState === WebSocket.OPEN);
                console.log("🔍 BROWSER WS: URL:", window.ws.url);
                
                if (window.ws.readyState === WebSocket.OPEN) {
                    console.log("✅ BROWSER WS: Connection is OPEN, testing send...");
                    
                    try {
                        // Test sending a simple screenshot_data message
                        const testData = {
                            type: 'screenshot_data',
                            filename: 'browser_ws_test.png',
                            dataURL: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChAGA4zNKmwAAAABJRU5ErkJggg==',
                            dimensions: { width: 1, height: 1 },
                            timestamp: Date.now(),
                            version: 'browser_ws_test',
                            source: 'browser_websocket_test'
                        };
                        
                        window.ws.send(JSON.stringify(testData));
                        console.log("✅ BROWSER WS: screenshot_data test message sent successfully!");
                        
                        return "BROWSER_WS_SEND_SUCCESS";
                        
                    } catch (error) {
                        console.error("❌ BROWSER WS: Send failed:", error.message);
                        return "BROWSER_WS_SEND_FAILED: " + error.message;
                    }
                } else {
                    console.error("❌ BROWSER WS: Connection not OPEN");
                    return "BROWSER_WS_NOT_OPEN";
                }
            } else {
                console.error("❌ BROWSER WS: window.ws object not found");
                return "BROWSER_WS_NOT_FOUND";
            }
            '''
            
            encoded_js = base64.b64encode(ws_test_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print("📤 Testing browser WebSocket connection...")
            await websocket.send(json.dumps(command))
            
            # Wait for response and check results
            for attempt in range(5):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=3)
                    result = json.loads(response)
                    
                    if result.get('type') == 'working':
                        continue
                        
                    elif result.get('type') == 'result':
                        data = result.get('data', {})
                        
                        if data.get('role') == 'BusCommand':
                            bus_result = data.get('result', {})
                            browser_response = bus_result.get('result', {}).get('browserResponse', {})
                            console_output = browser_response.get('output', [])
                            return_value = browser_response.get('result')
                            
                            print(f"\n📋 BROWSER WEBSOCKET TEST RESULTS:")
                            print(f"   Return value: {return_value}")
                            
                            # Analyze console output
                            ws_exists = any('window.ws exists: true' in msg.get('message', '') for msg in console_output)
                            ws_open = any('Connection is OPEN' in msg.get('message', '') for msg in console_output)
                            send_success = any('screenshot_data test message sent successfully' in msg.get('message', '') for msg in console_output)
                            send_failed = any('Send failed:' in msg.get('message', '') for msg in console_output)
                            
                            print(f"   WebSocket exists: {'✅ YES' if ws_exists else '❌ NO'}")
                            print(f"   WebSocket open: {'✅ YES' if ws_open else '❌ NO'}")
                            print(f"   Send test: {'✅ SUCCESS' if send_success else '❌ FAILED'}")
                            
                            # Show relevant console messages
                            print(f"\n📋 CONSOLE OUTPUT:")
                            for msg in console_output:
                                message = msg.get('message', '')
                                if 'BROWSER WS:' in message:
                                    level = msg.get('level', 'log').upper()
                                    print(f"      [{level}] {message}")
                            
                            # Check if screenshot_data was actually sent to server
                            if send_success:
                                print(f"\n🔍 Checking if server received screenshot_data...")
                                await asyncio.sleep(1)
                                
                                # Check for the test file
                                import os
                                test_file = ".continuum/screenshots/browser_ws_test.png"
                                
                                if os.path.exists(test_file):
                                    file_size = os.path.getsize(test_file)
                                    print(f"   ✅ SUCCESS: Server received and saved screenshot!")
                                    print(f"   📁 File: {test_file} ({file_size} bytes)")
                                    return True
                                else:
                                    print(f"   ❌ Server did not receive screenshot_data")
                                    return False
                            else:
                                print(f"\n❌ Browser WebSocket send failed")
                                return False
                        
                        elif data.get('task') == 'user_connection_greeting':
                            continue  # Skip greeting
                        else:
                            print(f"❌ Unexpected response: {data.get('role')}")
                            return False
                            
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout {attempt + 1}/5")
                    continue
            
            return False
            
    except Exception as e:
        print(f"❌ Test error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(test_browser_websocket())
    
    print(f"\n🎯 BROWSER WEBSOCKET TEST: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 MILESTONE 6: COMPLETE! Full async screenshot chain working!")
        print("✅ Browser WebSocket → Server API → File saved")
    else:
        print("🔧 Need to fix browser WebSocket connection for screenshot_data")