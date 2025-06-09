#!/usr/bin/env python3
"""
Debug what's really happening with screenshots
"""

import asyncio
import websockets
import json
import base64

async def debug_real_screenshot():
    print("🔍 DEBUGGING REAL SCREENSHOT ISSUES")
    print("=" * 50)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to Continuum")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Test the actual screenshot command that validation uses
            print("\n📸 Testing REAL screenshot command from validation...")
            
            screenshot_js = """
            console.log("📸 REAL SCREENSHOT TEST: Starting...");
            
            // This is the actual screenshot code from validation
            if (typeof html2canvas !== 'undefined') {
                console.log("📸 html2canvas is available");
                
                try {
                    html2canvas(document.body, {
                        allowTaint: true,
                        useCORS: true,
                        scale: 0.5,
                        backgroundColor: '#1a1a1a'
                    }).then(function(canvas) {
                        console.log("📸 Screenshot success! Canvas dimensions:", canvas.width, "x", canvas.height);
                        const dataURL = canvas.toDataURL('image/png');
                        console.log("📸 DataURL length:", dataURL.length);
                        
                        // Send back to server
                        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                            window.ws.send(JSON.stringify({
                                type: 'screenshot_data',
                                dataURL: dataURL.substring(0, 100) + "...", // Truncate for debug
                                filename: 'debug-test.png',
                                timestamp: Date.now(),
                                success: true
                            }));
                            console.log("📸 Screenshot data sent to server");
                        }
                        
                        return {
                            success: true,
                            width: canvas.width,
                            height: canvas.height,
                            dataLength: dataURL.length
                        };
                    }).catch(function(error) {
                        console.error("📸 Screenshot FAILED:", error);
                        console.error("📸 Error details:", error.message, error.stack);
                        return {
                            success: false,
                            error: error.message
                        };
                    });
                } catch (e) {
                    console.error("📸 Screenshot exception:", e);
                    return {
                        success: false,
                        error: e.message
                    };
                }
            } else {
                console.error("📸 html2canvas NOT AVAILABLE!");
                return {
                    success: false,
                    error: "html2canvas not loaded"
                };
            }
            
            console.log("📸 REAL SCREENSHOT TEST: Command sent");
            return "SCREENSHOT_TEST_INITIATED";
            """
            
            encoded = base64.b64encode(screenshot_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded}'
            }
            
            await websocket.send(json.dumps(command))
            
            # Wait for responses and show what actually happens
            print("\n📋 REAL-TIME DEBUG OUTPUT:")
            print("=" * 40)
            
            for attempt in range(10):  # Wait longer to see what happens
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=2)
                    result = json.loads(response)
                    
                    if result.get('type') == 'result':
                        bus_result = result.get('data', {}).get('result', {}).get('result', {})
                        browser_response = bus_result.get('browserResponse', {})
                        console_output = browser_response.get('output', [])
                        return_value = browser_response.get('result')
                        
                        print(f"\n📋 BUS COMMAND RESULT:")
                        print(f"  Return value: {return_value}")
                        
                        print(f"\n📋 CONSOLE MESSAGES ({len(console_output)}):")
                        for msg in console_output:
                            level = msg.get('level', 'unknown')
                            message = msg.get('message', '')
                            if level == 'error':
                                print(f"🚨 ERROR: {message}")
                            elif level == 'warn':
                                print(f"⚠️  WARN: {message}")
                            else:
                                print(f"📝 {level.upper()}: {message}")
                        
                        break
                        
                    elif result.get('type') == 'screenshot_data':
                        print(f"📸 SCREENSHOT DATA RECEIVED!")
                        print(f"  Filename: {result.get('filename')}")
                        print(f"  DataURL length: {len(result.get('dataURL', ''))}")
                        print(f"  Success: {result.get('success')}")
                        
                    elif result.get('type') == 'working':
                        print(f"⏳ Working: {result.get('data', '')}")
                        
                    else:
                        print(f"📥 Other: {result.get('type')} - {result}")
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout on attempt {attempt + 1}")
                    continue
            
            return True
            
    except Exception as e:
        print(f"❌ Debug error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(debug_real_screenshot())
    print(f"\n🎯 Debug completed: {'SUCCESS' if result else 'FAILED'}")