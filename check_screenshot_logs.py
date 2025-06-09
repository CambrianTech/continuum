#!/usr/bin/env python3
"""
Check what's happening with screenshot by reading console logs
"""
import asyncio
import websockets
import json
import base64

async def check_screenshot_logs():
    uri = "ws://localhost:9000"
    
    async with websockets.connect(uri) as websocket:
        await websocket.recv()
        await websocket.recv()
        
        # Simple test that logs everything
        test_js = """
        console.log("🔍 SCREENSHOT LOG CHECK: Starting simple test");
        
        if (typeof html2canvas === 'undefined') {
            console.log("🔍 html2canvas: NOT AVAILABLE");
            return "NO_HTML2CANVAS";
        } else {
            console.log("🔍 html2canvas: AVAILABLE");
        }
        
        if (typeof window.ws === 'undefined') {
            console.log("🔍 WebSocket: NOT AVAILABLE");
        } else {
            console.log("🔍 WebSocket: AVAILABLE, state =", window.ws.readyState);
        }
        
        // Create simple test element
        const testDiv = document.createElement('div');
        testDiv.style.cssText = 'width:100px;height:50px;background:red;color:white;padding:10px;';
        testDiv.textContent = 'TEST';
        testDiv.id = 'log-test-element';
        document.body.appendChild(testDiv);
        
        console.log("🔍 Created test element");
        
        // Try html2canvas
        console.log("🔍 Calling html2canvas...");
        
        html2canvas(testDiv, {
            allowTaint: true,
            scale: 0.5
        }).then(function(canvas) {
            console.log("✅ SCREENSHOT SUCCESS in log test!");
            console.log("✅ Canvas:", canvas.width + "x" + canvas.height);
            
            const dataURL = canvas.toDataURL('image/png');
            console.log("✅ DataURL length:", dataURL.length);
            
            // Try WebSocket send
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                console.log("✅ Sending via WebSocket...");
                window.ws.send(JSON.stringify({
                    type: 'log_test_screenshot',
                    width: canvas.width,
                    height: canvas.height,
                    success: true
                }));
                console.log("✅ WebSocket send completed");
            } else {
                console.log("❌ WebSocket not ready for send");
            }
            
            // Cleanup
            testDiv.remove();
            console.log("✅ Test cleanup done");
            
        }).catch(function(error) {
            console.log("❌ SCREENSHOT ERROR in log test:", error.message);
            console.log("❌ Error type:", error.constructor.name);
            
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                window.ws.send(JSON.stringify({
                    type: 'log_test_screenshot_error',
                    error: error.message,
                    success: false
                }));
            }
            
            testDiv.remove();
        });
        
        console.log("🔍 SCREENSHOT LOG CHECK: Test initiated");
        return "LOG_CHECK_STARTED";
        """
        
        encoded = base64.b64encode(test_js.encode()).decode()
        command = {
            'type': 'task',
            'role': 'system',
            'task': f'[CMD:BROWSER_JS] {encoded}'
        }
        
        print("📝 Sending screenshot log check...")
        await websocket.send(json.dumps(command))
        
        # Wait for both console logs AND WebSocket messages
        for attempt in range(8):
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=2)
                data = json.loads(response)
                
                if data.get('type') == 'result':
                    # Check console output for logs
                    try:
                        result_data = data.get('data', {})
                        inner_result = result_data.get('result', {})
                        browser_result = inner_result.get('result', {})
                        browser_response = browser_result.get('browserResponse', {})
                        console_output = browser_response.get('output', [])
                        
                        print(f"\n📋 CONSOLE LOGS FROM SCREENSHOT TEST:")
                        for msg in console_output:
                            message = msg.get('message', '')
                            if '🔍' in message or '✅' in message or '❌' in message:
                                print(f"   {message}")
                        
                    except Exception as e:
                        print(f"Error reading console: {e}")
                        
                elif data.get('type') == 'log_test_screenshot':
                    print(f"✅ SUCCESS: Screenshot WebSocket message received!")
                    print(f"   Dimensions: {data.get('width')}x{data.get('height')}")
                    break
                    
                elif data.get('type') == 'log_test_screenshot_error':
                    print(f"❌ ERROR: {data.get('error')}")
                    break
                    
                elif data.get('type') == 'working':
                    continue
                    
            except asyncio.TimeoutError:
                print(f"⏰ Waiting... {attempt + 1}/8")
                continue
        
        return True

if __name__ == "__main__":
    asyncio.run(check_screenshot_logs())