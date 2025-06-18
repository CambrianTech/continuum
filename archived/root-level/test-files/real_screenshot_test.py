#!/usr/bin/env python3
"""
Real screenshot test that actually tests screenshot functionality
"""

import asyncio
import websockets
import json
import base64

async def real_screenshot_test():
    print("📸 REAL SCREENSHOT VALIDATION TEST")
    print("=" * 50)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to Continuum")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Test actual screenshot command with proper validation
            print("\n📸 Step 1: Test screenshot prerequisites...")
            
            prereq_js = """
            console.log("📋 SCREENSHOT PREREQUISITES CHECK");
            
            const results = {
                html2canvas: typeof html2canvas !== 'undefined',
                websocket: typeof window.ws !== 'undefined' && window.ws.readyState === WebSocket.OPEN,
                canvasSupport: !!document.createElement('canvas').getContext,
                bodyExists: !!document.body
            };
            
            console.log("Prerequisites:", results);
            
            if (!results.html2canvas) {
                console.error("❌ CRITICAL: html2canvas not loaded!");
            }
            if (!results.websocket) {
                console.error("❌ CRITICAL: WebSocket not available!");
            }
            
            return results;
            """
            
            encoded = base64.b64encode(prereq_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded}'
            }
            
            await websocket.send(json.dumps(command))
            
            # Get prerequisites result
            prereq_result = None
            for attempt in range(3):
                response = await websocket.recv()
                current_result = json.loads(response)
                
                if current_result.get('type') == 'result':
                    prereq_result = current_result
                    break
                elif current_result.get('type') == 'working':
                    continue
            
            if prereq_result:
                # Parse the result properly
                try:
                    bus_result = prereq_result.get('data', {}).get('result', {}).get('result', {})
                    browser_response = bus_result.get('browserResponse', {})
                    return_value = browser_response.get('result')
                    console_output = browser_response.get('output', [])
                    
                    print(f"\n📋 PREREQUISITES RESULT:")
                    if return_value:
                        try:
                            if isinstance(return_value, str):
                                status = json.loads(return_value)
                            else:
                                status = return_value
                            print(f"  html2canvas: {status.get('html2canvas', 'unknown')}")
                            print(f"  websocket: {status.get('websocket', 'unknown')}")
                            print(f"  canvas support: {status.get('canvasSupport', 'unknown')}")
                            print(f"  body exists: {status.get('bodyExists', 'unknown')}")
                        except:
                            print(f"  Raw result: {return_value}")
                    
                    print(f"\n📋 CONSOLE OUTPUT ({len(console_output)}):")
                    for msg in console_output:
                        level = msg.get('level', 'unknown')
                        message = msg.get('message', '')
                        if level == 'error':
                            print(f"🚨 ERROR: {message}")
                        else:
                            print(f"📝 {level.upper()}: {message}")
                            
                except Exception as e:
                    print(f"❌ Error parsing prerequisites: {e}")
            
            # Now test actual screenshot
            print(f"\n📸 Step 2: Test actual screenshot functionality...")
            
            screenshot_js = """
            console.log("📸 ACTUAL SCREENSHOT TEST");
            
            if (typeof html2canvas === 'undefined') {
                console.error("❌ Cannot test screenshot - html2canvas missing");
                return { success: false, error: "html2canvas_missing" };
            }
            
            console.log("📸 Starting html2canvas capture...");
            
            // This should be synchronous test that returns immediately
            let testResult = { success: false, error: "unknown" };
            
            try {
                // Test with minimal options first
                html2canvas(document.body, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 0.1,
                    width: 100,
                    height: 100
                }).then(function(canvas) {
                    console.log("✅ SCREENSHOT SUCCESS! Canvas:", canvas.width + "x" + canvas.height);
                    
                    // Send success via WebSocket if available
                    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                        window.ws.send(JSON.stringify({
                            type: 'screenshot_validation_success',
                            width: canvas.width,
                            height: canvas.height,
                            timestamp: Date.now()
                        }));
                    }
                    
                }).catch(function(error) {
                    console.error("❌ SCREENSHOT FAILED:", error.message);
                    console.error("❌ Error details:", error);
                    
                    // Send error via WebSocket if available
                    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                        window.ws.send(JSON.stringify({
                            type: 'screenshot_validation_error',
                            error: error.message,
                            timestamp: Date.now()
                        }));
                    }
                });
                
                // Return immediately for synchronous result
                return { 
                    success: true, 
                    message: "Screenshot test initiated - check WebSocket for async result" 
                };
                
            } catch (e) {
                console.error("❌ SCREENSHOT EXCEPTION:", e.message);
                return { success: false, error: e.message };
            }
            """
            
            encoded_screenshot = base64.b64encode(screenshot_js.encode()).decode()
            screenshot_command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_screenshot}'
            }
            
            await websocket.send(json.dumps(screenshot_command))
            
            # Wait for both the command result AND the async screenshot result
            screenshot_success = False
            command_result = None
            
            for attempt in range(10):  # Wait longer for async results
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=2)
                    current_result = json.loads(response)
                    
                    if current_result.get('type') == 'result':
                        command_result = current_result
                        print("📥 Got command result")
                        
                    elif current_result.get('type') == 'screenshot_validation_success':
                        screenshot_success = True
                        print(f"✅ SCREENSHOT SUCCESS! {current_result.get('width')}x{current_result.get('height')}")
                        break
                        
                    elif current_result.get('type') == 'screenshot_validation_error':
                        print(f"❌ SCREENSHOT ERROR: {current_result.get('error')}")
                        break
                        
                    elif current_result.get('type') == 'working':
                        continue
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Waiting for screenshot result... ({attempt + 1}/10)")
                    continue
            
            # Show final results
            print(f"\n📊 FINAL SCREENSHOT VALIDATION:")
            print(f"  Command sent: ✅")
            print(f"  Command result received: {'✅' if command_result else '❌'}")
            print(f"  Screenshot success: {'✅' if screenshot_success else '❌'}")
            
            if command_result:
                # Show console output from the command
                try:
                    bus_result = command_result.get('data', {}).get('result', {}).get('result', {})
                    browser_response = bus_result.get('browserResponse', {})
                    console_output = browser_response.get('output', [])
                    
                    print(f"\n📋 SCREENSHOT COMMAND CONSOLE ({len(console_output)}):")
                    for msg in console_output:
                        level = msg.get('level', 'unknown')
                        message = msg.get('message', '')
                        if level == 'error':
                            print(f"🚨 ERROR: {message}")
                        elif level == 'warn':
                            print(f"⚠️  WARN: {message}")
                        else:
                            print(f"📝 {level.upper()}: {message}")
                            
                except Exception as e:
                    print(f"❌ Error parsing command result: {e}")
            
            return screenshot_success
            
    except Exception as e:
        print(f"❌ Test error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(real_screenshot_test())
    print(f"\n🎯 REAL SCREENSHOT VALIDATION: {'SUCCESS' if result else 'FAILED'}")