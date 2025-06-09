#!/usr/bin/env python3
"""
Debug M6 by checking console output directly
"""
import asyncio
import websockets
import json
import base64

async def debug_m6_console():
    print("🔍 DEBUG M6: Checking console output for screenshot execution")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to debug M6")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Simple M6 test with detailed logging
            m6_debug_js = '''
            console.log("📸 M6 DEBUG: Starting detailed test");
            console.log("📸 M6 DEBUG: html2canvas type:", typeof html2canvas);
            console.log("📸 M6 DEBUG: WebSocket type:", typeof window.ws);
            console.log("📸 M6 DEBUG: WebSocket readyState:", window.ws ? window.ws.readyState : "no ws");
            
            // Create test element
            const testElement = document.createElement('div');
            testElement.style.cssText = 'width:150px;height:60px;background:#ff6600;color:white;padding:15px;border-radius:8px;font-family:monospace;position:fixed;top:20px;left:20px;z-index:10000;text-align:center;font-weight:bold;';
            testElement.innerHTML = '<div>🔍 M6 DEBUG</div><div>v0.2.1983</div>';
            testElement.id = 'debug-m6-element';
            document.body.appendChild(testElement);
            console.log("📸 M6 DEBUG: Test element created and added to DOM");
            
            if (typeof html2canvas !== 'undefined') {
                console.log("📸 M6 DEBUG: Calling html2canvas...");
                
                html2canvas(testElement, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 1
                }).then(function(canvas) {
                    console.log("✅ M6 DEBUG: html2canvas SUCCESS!");
                    console.log("✅ M6 DEBUG: Canvas size:", canvas.width + "x" + canvas.height);
                    
                    const dataURL = canvas.toDataURL('image/png');
                    console.log("✅ M6 DEBUG: DataURL length:", dataURL.length);
                    console.log("✅ M6 DEBUG: DataURL preview:", dataURL.substring(0, 50) + "...");
                    
                    const timestamp = Date.now();
                    const filename = `debug_m6_${timestamp}.png`;
                    console.log("✅ M6 DEBUG: Generated filename:", filename);
                    
                    // Try WebSocket save
                    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                        console.log("💾 M6 DEBUG: WebSocket ready, sending save request...");
                        
                        try {
                            window.ws.send(JSON.stringify({
                                type: 'screenshot_data',
                                filename: filename,
                                dataURL: dataURL,
                                dimensions: {
                                    width: canvas.width,
                                    height: canvas.height
                                },
                                timestamp: timestamp,
                                version: "debug_v0.2.1983",
                                source: 'm6_debug'
                            }));
                            console.log("✅ M6 DEBUG: WebSocket send completed successfully");
                        } catch(wsError) {
                            console.error("❌ M6 DEBUG: WebSocket send failed:", wsError.message);
                        }
                    } else {
                        console.error("❌ M6 DEBUG: WebSocket not ready:", window.ws ? window.ws.readyState : "no ws object");
                    }
                    
                    // Clean up
                    testElement.remove();
                    console.log("📸 M6 DEBUG: Cleanup completed");
                    
                }).catch(function(error) {
                    console.error("❌ M6 DEBUG: html2canvas FAILED:", error.message);
                    console.error("❌ M6 DEBUG: Error stack:", error.stack);
                    testElement.remove();
                });
                
            } else {
                console.error("❌ M6 DEBUG: html2canvas is NOT available");
                testElement.remove();
            }
            
            console.log("📸 M6 DEBUG: Test initiated, waiting for results...");
            return "M6_DEBUG_INITIATED";
            '''
            
            encoded_js = base64.b64encode(m6_debug_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system', 
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print("📤 Sending M6 debug command...")
            await websocket.send(json.dumps(command))
            
            # Wait for and capture the response with console output
            for attempt in range(10):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=2)
                    result = json.loads(response)
                    
                    print(f"\n📨 Response {attempt + 1}:")
                    print(f"   Type: {result.get('type')}")
                    
                    if result.get('type') == 'result':
                        bus_result = result.get('data', {}).get('result', {}).get('result', {})
                        browser_response = bus_result.get('browserResponse', {})
                        console_output = browser_response.get('output', [])
                        
                        print(f"   Bus executed: {bus_result.get('executed')}")
                        print(f"   Browser success: {browser_response.get('success')}")
                        print(f"   Console messages: {len(console_output)}")
                        print(f"   Return value: {browser_response.get('result')}")
                        
                        print(f"\n📋 CONSOLE OUTPUT:")
                        for i, msg in enumerate(console_output):
                            level = msg.get('level', 'log')
                            message = msg.get('message', '')
                            print(f"   {i+1}. [{level.upper()}] {message}")
                        
                        # Check for screenshot success indicators
                        screenshot_success = any('html2canvas SUCCESS' in msg.get('message', '') for msg in console_output)
                        websocket_sent = any('WebSocket send completed' in msg.get('message', '') for msg in console_output)
                        
                        print(f"\n🔍 M6 DEBUG ANALYSIS:")
                        print(f"   Screenshot captured: {'✅ YES' if screenshot_success else '❌ NO'}")
                        print(f"   WebSocket save sent: {'✅ YES' if websocket_sent else '❌ NO'}")
                        
                        return True
                        
                    elif result.get('type') == 'working':
                        print("   Status: Working...")
                        continue
                        
                except asyncio.TimeoutError:
                    print(f"   Timeout {attempt + 1}/10")
                    continue
            
            print("❌ No response received")
            return False
            
    except Exception as e:
        print(f"❌ Debug error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(debug_m6_console())
    print(f"\n🎯 M6 DEBUG RESULT: {'SUCCESS' if result else 'FAILED'}")