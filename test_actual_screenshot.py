#!/usr/bin/env python3
"""
Test that actually validates screenshot works by checking console errors
"""

import asyncio
import websockets
import json
import base64

async def test_actual_screenshot():
    print("📸 ACTUAL SCREENSHOT ERROR TEST")
    print("=" * 40)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to Continuum")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Send a screenshot command and check for createPattern errors
            screenshot_test = """
            console.log("📸 TESTING FOR createPattern ERRORS");
            
            // Clear any previous errors
            const errors = [];
            const originalError = console.error;
            
            console.error = function(...args) {
                const errorMsg = args.join(' ');
                errors.push(errorMsg);
                originalError.apply(console, args);
            };
            
            try {
                // Attempt screenshot that might trigger createPattern error
                html2canvas(document.body, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 0.5
                }).then(function(canvas) {
                    console.log("✅ SCREENSHOT SUCCESS - no createPattern errors!");
                    console.error = originalError; // Restore
                    
                    return {
                        success: true,
                        errors: errors,
                        canvasSize: canvas.width + "x" + canvas.height
                    };
                    
                }).catch(function(error) {
                    console.error("❌ SCREENSHOT ERROR:", error.message);
                    console.error = originalError; // Restore
                    
                    return {
                        success: false,
                        errors: errors,
                        mainError: error.message
                    };
                });
                
                // Return immediately with captured errors so far
                setTimeout(() => {
                    console.error = originalError; // Restore after timeout
                }, 5000);
                
                return {
                    testStarted: true,
                    initialErrors: errors.length,
                    message: "Screenshot test started - check for createPattern errors"
                };
                
            } catch (e) {
                console.error = originalError; // Restore
                console.error("❌ Exception in screenshot test:", e.message);
                return {
                    success: false,
                    exception: e.message,
                    errors: errors
                };
            }
            """
            
            encoded = base64.b64encode(screenshot_test.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded}'
            }
            
            print("📤 Sending screenshot error test...")
            await websocket.send(json.dumps(command))
            
            # Wait for result and check for createPattern errors
            for attempt in range(5):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=3)
                    data = json.loads(response)
                    
                    if data.get('type') == 'result':
                        # Parse the nested result structure
                        try:
                            result_data = data.get('data', {})
                            inner_result = result_data.get('result', {})
                            browser_result = inner_result.get('result', {})
                            browser_response = browser_result.get('browserResponse', {})
                            console_output = browser_response.get('output', [])
                            return_value = browser_response.get('result')
                            
                            print(f"\n📋 SCREENSHOT TEST RESULT:")
                            if return_value:
                                try:
                                    if isinstance(return_value, str):
                                        result_obj = json.loads(return_value)
                                    else:
                                        result_obj = return_value
                                    print(f"  Test started: {result_obj.get('testStarted', 'unknown')}")
                                    print(f"  Initial errors: {result_obj.get('initialErrors', 'unknown')}")
                                    print(f"  Message: {result_obj.get('message', 'none')}")
                                except:
                                    print(f"  Raw result: {return_value}")
                            
                            print(f"\n📋 CONSOLE OUTPUT ANALYSIS:")
                            createPattern_errors = 0
                            other_errors = 0
                            
                            for msg in console_output:
                                level = msg.get('level', 'unknown')
                                message = msg.get('message', '')
                                
                                if level == 'error':
                                    if 'createPattern' in message:
                                        createPattern_errors += 1
                                        print(f"🚨 createPattern ERROR: {message}")
                                    else:
                                        other_errors += 1
                                        print(f"❌ OTHER ERROR: {message}")
                                elif level == 'warn':
                                    print(f"⚠️  WARN: {message}")
                                else:
                                    print(f"📝 {level.upper()}: {message}")
                            
                            print(f"\n📊 ERROR SUMMARY:")
                            print(f"  createPattern errors: {createPattern_errors}")
                            print(f"  Other errors: {other_errors}")
                            print(f"  Total console messages: {len(console_output)}")
                            
                            # Determine if screenshot actually works
                            screenshot_works = createPattern_errors == 0
                            print(f"\n🎯 ACTUAL SCREENSHOT STATUS: {'✅ WORKING' if screenshot_works else '❌ FAILING'}")
                            
                            if createPattern_errors > 0:
                                print("💡 DIAGNOSIS: createPattern errors indicate zero-dimension canvas elements are still being processed")
                                print("💡 SOLUTION NEEDED: More aggressive canvas element filtering required")
                            
                            return screenshot_works
                            
                        except Exception as e:
                            print(f"❌ Error parsing result: {e}")
                            return False
                            
                    elif data.get('type') == 'working':
                        print("⏳ Processing...")
                        continue
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout {attempt + 1}/5")
                    continue
            
            print("❌ No result received")
            return False
            
    except Exception as e:
        print(f"❌ Test error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(test_actual_screenshot())
    print(f"\n🎯 FINAL RESULT: {'SCREENSHOT WORKS' if result else 'SCREENSHOT BROKEN'}")