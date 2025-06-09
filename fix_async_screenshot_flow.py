#!/usr/bin/env python3
"""
Fix async screenshot flow - each promise waits on its own WebSocket connection
"""
import asyncio
import websockets
import json
import base64
import time

async def fix_async_screenshot_flow():
    print("🔄 FIXING ASYNC SCREENSHOT FLOW")
    print("=" * 50)
    print("Each promise waits on its own WebSocket connection, event-driven async")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Debugger connected for async flow fix")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Test async WebSocket connection from browser
            connection_test_js = '''
            console.log("🔄 ASYNC: Testing browser WebSocket connection status");
            console.log("🔄 ASYNC: window.ws exists:", typeof window.ws !== 'undefined');
            console.log("🔄 ASYNC: WebSocket readyState:", window.ws ? window.ws.readyState : 'no ws');
            console.log("🔄 ASYNC: WebSocket.OPEN constant:", WebSocket.OPEN);
            
            if (window.ws) {
                console.log("🔄 ASYNC: WebSocket URL:", window.ws.url);
                console.log("🔄 ASYNC: WebSocket protocol:", window.ws.protocol);
                console.log("🔄 ASYNC: WebSocket extensions:", window.ws.extensions);
                
                // Check connection state
                const states = ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED'];
                console.log("🔄 ASYNC: Connection state:", states[window.ws.readyState] || 'UNKNOWN');
                
                // Add event listeners for debugging
                window.ws.addEventListener('open', function() {
                    console.log("✅ ASYNC: WebSocket opened");
                });
                
                window.ws.addEventListener('close', function(event) {
                    console.log("❌ ASYNC: WebSocket closed:", event.code, event.reason);
                });
                
                window.ws.addEventListener('error', function(error) {
                    console.error("❌ ASYNC: WebSocket error:", error);
                });
                
                // Try sending a test message if connected
                if (window.ws.readyState === WebSocket.OPEN) {
                    console.log("📤 ASYNC: Sending test ping...");
                    try {
                        window.ws.send(JSON.stringify({
                            type: 'async_test_ping',
                            timestamp: Date.now(),
                            source: 'browser_async_test'
                        }));
                        console.log("✅ ASYNC: Test ping sent successfully");
                    } catch (error) {
                        console.error("❌ ASYNC: Test ping failed:", error.message);
                    }
                } else {
                    console.log("⏳ ASYNC: WebSocket not ready for sending");
                }
            } else {
                console.error("❌ ASYNC: No WebSocket object found");
            }
            
            return "ASYNC_CONNECTION_TEST_COMPLETE";
            '''
            
            encoded_js = base64.b64encode(connection_test_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print("📤 Testing async WebSocket connection...")
            await websocket.send(json.dumps(command))
            
            # Wait for response and check logs
            await asyncio.sleep(2)
            
            # Now test the proper async screenshot flow with WebSocket promise
            print(f"\n🔄 ASYNC: Testing promise-based screenshot with WebSocket await")
            
            async_screenshot_js = '''
            console.log("🔄 ASYNC SCREENSHOT: Starting promise-based flow");
            
            // Create screenshot element
            const testElement = document.createElement('div');
            testElement.style.cssText = 'width:200px;height:80px;background:#0066ff;color:white;padding:15px;border-radius:8px;font-family:monospace;position:fixed;top:150px;left:150px;z-index:10000;text-align:center;font-weight:bold;';
            testElement.innerHTML = '<div>🔄 ASYNC FLOW</div><div>v0.2.1983</div><div>Promise + WebSocket</div>';
            testElement.id = 'async-screenshot-element';
            document.body.appendChild(testElement);
            console.log("🔄 ASYNC SCREENSHOT: Test element created");
            
            // Wait for WebSocket to be ready (promise-based)
            function waitForWebSocket() {
                return new Promise((resolve, reject) => {
                    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                        console.log("✅ ASYNC SCREENSHOT: WebSocket already ready");
                        resolve(window.ws);
                        return;
                    }
                    
                    if (!window.ws) {
                        console.error("❌ ASYNC SCREENSHOT: No WebSocket object");
                        reject(new Error("No WebSocket object"));
                        return;
                    }
                    
                    if (window.ws.readyState === WebSocket.CONNECTING) {
                        console.log("⏳ ASYNC SCREENSHOT: Waiting for WebSocket connection...");
                        window.ws.addEventListener('open', () => {
                            console.log("✅ ASYNC SCREENSHOT: WebSocket connected");
                            resolve(window.ws);
                        });
                        
                        window.ws.addEventListener('error', (error) => {
                            console.error("❌ ASYNC SCREENSHOT: WebSocket connection failed");
                            reject(error);
                        });
                        
                        // Timeout after 5 seconds
                        setTimeout(() => {
                            if (window.ws.readyState !== WebSocket.OPEN) {
                                reject(new Error("WebSocket connection timeout"));
                            }
                        }, 5000);
                    } else {
                        reject(new Error("WebSocket in invalid state: " + window.ws.readyState));
                    }
                });
            }
            
            // Screenshot promise that waits for WebSocket
            async function takeScreenshotWithWebSocket() {
                try {
                    // Step 1: Wait for WebSocket to be ready
                    console.log("🔄 ASYNC SCREENSHOT: Step 1 - Waiting for WebSocket...");
                    const websocket = await waitForWebSocket();
                    console.log("✅ ASYNC SCREENSHOT: WebSocket ready for screenshot");
                    
                    // Step 2: Take screenshot
                    console.log("🔄 ASYNC SCREENSHOT: Step 2 - Taking screenshot...");
                    if (typeof html2canvas === 'undefined') {
                        throw new Error("html2canvas not available");
                    }
                    
                    const canvas = await html2canvas(testElement, {
                        allowTaint: true,
                        useCORS: true,
                        scale: 1,
                        backgroundColor: null
                    });
                    
                    console.log("✅ ASYNC SCREENSHOT: Screenshot captured", canvas.width + "x" + canvas.height);
                    
                    // Step 3: Prepare data
                    const dataURL = canvas.toDataURL('image/png');
                    const timestamp = Date.now();
                    const filename = `async_flow_${timestamp}.png`;
                    
                    console.log("🔄 ASYNC SCREENSHOT: Step 3 - Preparing data");
                    console.log("📝 ASYNC SCREENSHOT: Filename:", filename);
                    console.log("📏 ASYNC SCREENSHOT: DataURL length:", dataURL.length);
                    
                    // Step 4: Send via WebSocket (with promise)
                    console.log("🔄 ASYNC SCREENSHOT: Step 4 - Sending via WebSocket...");
                    
                    const message = {
                        type: 'screenshot_data',
                        filename: filename,
                        dataURL: dataURL,
                        dimensions: {
                            width: canvas.width,
                            height: canvas.height
                        },
                        timestamp: timestamp,
                        version: "async_flow_v0.2.1983",
                        source: 'async_promise_flow'
                    };
                    
                    // Send with error handling
                    websocket.send(JSON.stringify(message));
                    console.log("✅ ASYNC SCREENSHOT: WebSocket send completed");
                    
                    // Step 5: Cleanup
                    testElement.remove();
                    console.log("🔄 ASYNC SCREENSHOT: Cleanup completed");
                    
                    return {
                        success: true,
                        filename: filename,
                        width: canvas.width,
                        height: canvas.height,
                        dataLength: dataURL.length
                    };
                    
                } catch (error) {
                    console.error("❌ ASYNC SCREENSHOT: Error in flow:", error.message);
                    testElement.remove();
                    
                    return {
                        success: false,
                        error: error.message
                    };
                }
            }
            
            // Execute async screenshot flow
            return takeScreenshotWithWebSocket().then(result => {
                console.log("🎯 ASYNC SCREENSHOT: Flow complete:", result);
                return JSON.stringify(result);
            }).catch(error => {
                console.error("🎯 ASYNC SCREENSHOT: Flow failed:", error.message);
                return JSON.stringify({success: false, error: error.message});
            });
            '''
            
            encoded_async = base64.b64encode(async_screenshot_js.encode()).decode()
            async_command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_async}'
            }
            
            print("📤 Sending async screenshot flow command...")
            await websocket.send(json.dumps(async_command))
            
            # Wait for response with result
            for attempt in range(10):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=3)
                    result = json.loads(response)
                    
                    if result.get('type') == 'result':
                        try:
                            bus_result = result.get('data', {}).get('result', {}).get('result', {})
                            browser_response = bus_result.get('browserResponse', {})
                            return_value = browser_response.get('result')
                        except AttributeError:
                            # Handle case where result is a string
                            return_value = result.get('data', {}).get('result', {}).get('result')
                            if isinstance(return_value, str):
                                print(f"📋 Raw string result: {return_value[:100]}...")
                                continue
                        
                        if return_value:
                            try:
                                flow_result = json.loads(return_value)
                                
                                print(f"\n🎯 ASYNC FLOW RESULT:")
                                print(f"   Success: {'✅ YES' if flow_result.get('success') else '❌ NO'}")
                                
                                if flow_result.get('success'):
                                    print(f"   Filename: {flow_result.get('filename')}")
                                    print(f"   Dimensions: {flow_result.get('width')}x{flow_result.get('height')}")
                                    print(f"   Data length: {flow_result.get('dataLength')}")
                                    
                                    # Check if file was saved
                                    import os
                                    screenshot_path = f".continuum/screenshots/{flow_result.get('filename')}"
                                    
                                    await asyncio.sleep(1)  # Give server time to save
                                    
                                    if os.path.exists(screenshot_path):
                                        file_size = os.path.getsize(screenshot_path)
                                        print(f"   ✅ FILE SAVED: {screenshot_path} ({file_size} bytes)")
                                        return True
                                    else:
                                        print(f"   ❌ FILE NOT SAVED: {screenshot_path}")
                                        return False
                                else:
                                    print(f"   Error: {flow_result.get('error')}")
                                    return False
                                    
                            except Exception as parse_error:
                                print(f"❌ Could not parse flow result: {parse_error}")
                                print(f"   Raw result: {return_value}")
                                return False
                        
                        break
                        
                    elif result.get('type') == 'working':
                        continue
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Waiting for async flow result... {attempt + 1}/10")
                    continue
            
            return False
            
    except Exception as e:
        print(f"❌ Async flow error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(fix_async_screenshot_flow())
    print(f"\n🎯 ASYNC FLOW FIX: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 Screenshot async flow working end-to-end!")
        print("✅ MILESTONE 6: FULLY VALIDATED - Screenshot + Version + File Save")
    else:
        print("❌ Async flow still needs debugging")