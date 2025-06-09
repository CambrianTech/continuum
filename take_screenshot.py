#!/usr/bin/env python3
"""
Take a screenshot using the fixed validation system
"""
import asyncio
import websockets
import json
import base64
import time

async def take_screenshot():
    print("📸 TAKING SCREENSHOT - Using Fixed Validation System")
    print("=" * 60)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to Continuum")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Use the working screenshot code from validation
            screenshot_js = """
            console.log("📸 SCREENSHOT: Starting capture with working validation system...");
            
            // Find version badge or create a target element  
            let targetElement = document.querySelector('.version-badge, [class*="version"]');
            if (!targetElement) {
                // Create a nice screenshot target
                targetElement = document.createElement('div');
                targetElement.style.cssText = `
                    width: 200px;
                    height: 60px;
                    background: linear-gradient(135deg, #0066cc, #004499);
                    color: white;
                    padding: 15px;
                    border-radius: 10px;
                    font-family: 'Monaco', 'Consolas', monospace;
                    font-size: 16px;
                    font-weight: bold;
                    text-align: center;
                    box-shadow: 0 4px 15px rgba(0,102,204,0.3);
                    border: 2px solid #0088ff;
                    position: fixed;
                    top: 50px;
                    left: 50px;
                    z-index: 10000;
                `;
                targetElement.innerHTML = `
                    <div>✅ CONTINUUM</div>
                    <div style="font-size: 12px; opacity: 0.9;">v0.2.1983</div>
                    <div style="font-size: 10px; opacity: 0.7;">Screenshot Test</div>
                `;
                targetElement.id = 'screenshot-target-element';
                document.body.appendChild(targetElement);
                console.log("📸 Created beautiful screenshot target");
            }
            
            console.log("📸 Target element selected:", targetElement.tagName, targetElement.id);
            
            if (typeof html2canvas === 'undefined') {
                throw new Error("html2canvas not available for screenshot");
            }
            
            // Remove ALL canvas elements to prevent createPattern errors
            const allCanvases = document.querySelectorAll('canvas');
            const removedCanvases = [];
            
            console.log("📸 Removing", allCanvases.length, "canvas elements to prevent createPattern errors");
            
            allCanvases.forEach((canvas, i) => {
                console.log("📸 Removing canvas", i + ":", canvas.width + "x" + canvas.height);
                removedCanvases.push({
                    element: canvas,
                    parent: canvas.parentNode,
                    nextSibling: canvas.nextSibling
                });
                canvas.remove();
            });
            
            console.log("📸 Starting html2canvas capture...");
            
            return html2canvas(targetElement, {
                allowTaint: true,
                useCORS: true,
                scale: 2,  // Higher quality
                backgroundColor: null
            }).then(function(canvas) {
                // Restore removed canvas elements
                console.log("📸 Restoring", removedCanvases.length, "canvas elements");
                removedCanvases.forEach((item, i) => {
                    if (item.parent) {
                        if (item.nextSibling) {
                            item.parent.insertBefore(item.element, item.nextSibling);
                        } else {
                            item.parent.appendChild(item.element);
                        }
                    }
                });
                
                // Clean up test element if we created it
                const testElement = document.getElementById('screenshot-target-element');
                if (testElement) {
                    testElement.remove();
                    console.log("📸 Cleaned up test element");
                }
                
                console.log("✅ SCREENSHOT SUCCESS!");
                console.log("✅ Canvas dimensions:", canvas.width + "x" + canvas.height);
                
                // Convert to base64 data
                const dataURL = canvas.toDataURL('image/png');
                console.log("✅ DataURL length:", dataURL.length);
                console.log("✅ DataURL preview:", dataURL.substring(0, 50) + "...");
                
                return {
                    success: true,
                    width: canvas.width,
                    height: canvas.height,
                    dataLength: dataURL.length,
                    dataURL: dataURL,
                    timestamp: Date.now(),
                    message: "Screenshot captured successfully using fixed validation system"
                };
                
            }).catch(function(error) {
                // Restore removed canvas elements even on error
                console.log("📸 Error occurred, restoring", removedCanvases.length, "canvas elements");
                removedCanvases.forEach((item, i) => {
                    if (item.parent) {
                        if (item.nextSibling) {
                            item.parent.insertBefore(item.element, item.nextSibling);
                        } else {
                            item.parent.appendChild(item.element);
                        }
                    }
                });
                
                // Clean up test element
                const testElement = document.getElementById('screenshot-target-element');
                if (testElement) {
                    testElement.remove();
                }
                
                console.error("❌ Screenshot failed:", error.message);
                
                return {
                    success: false,
                    error: error.message,
                    errorType: error.constructor.name,
                    message: "Screenshot failed"
                };
            });
            """
            
            encoded = base64.b64encode(screenshot_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded}'
            }
            
            print("📤 Sending screenshot command...")
            await websocket.send(json.dumps(command))
            
            # Wait for result
            for attempt in range(5):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=5)
                    result = json.loads(response)
                    
                    if result.get('type') == 'result':
                        try:
                            bus_result = result.get('data', {}).get('result', {}).get('result', {})
                            browser_response = bus_result.get('browserResponse', {})
                            return_value = browser_response.get('result')
                            console_output = browser_response.get('output', [])
                            
                            print(f"\n📋 SCREENSHOT RESULT:")
                            print("=" * 40)
                            
                            # Show console output
                            for msg in console_output:
                                message = msg.get('message', '')
                                if '📸' in message or '✅' in message or '❌' in message:
                                    print(f"   {message}")
                            
                            if return_value:
                                try:
                                    if isinstance(return_value, str):
                                        result_obj = json.loads(return_value)
                                    else:
                                        result_obj = return_value
                                    
                                    if result_obj.get('success'):
                                        print(f"\n🎉 SCREENSHOT TAKEN SUCCESSFULLY!")
                                        print(f"   📏 Dimensions: {result_obj.get('width')}x{result_obj.get('height')}")
                                        print(f"   💾 Data size: {result_obj.get('dataLength')} bytes")
                                        print(f"   🕐 Timestamp: {result_obj.get('timestamp')}")
                                        
                                        # Save the screenshot data
                                        dataURL = result_obj.get('dataURL', '')
                                        if dataURL:
                                            # Extract base64 data
                                            if 'base64,' in dataURL:
                                                base64_data = dataURL.split('base64,')[1]
                                                
                                                # Save to file
                                                import base64 as b64
                                                screenshot_data = b64.b64decode(base64_data)
                                                
                                                timestamp = str(int(time.time()))
                                                filename = f"continuum_screenshot_{timestamp}.png"
                                                
                                                with open(filename, 'wb') as f:
                                                    f.write(screenshot_data)
                                                
                                                print(f"   💾 Saved to: {filename}")
                                                print(f"   📂 File size: {len(screenshot_data)} bytes")
                                        
                                        return True
                                    else:
                                        print(f"\n❌ SCREENSHOT FAILED:")
                                        print(f"   Error: {result_obj.get('error')}")
                                        print(f"   Type: {result_obj.get('errorType')}")
                                        return False
                                        
                                except Exception as e:
                                    print(f"❌ Error parsing result: {e}")
                                    print(f"   Raw result: {str(return_value)[:200]}...")
                                    return False
                            
                            break
                            
                        except Exception as e:
                            print(f"❌ Error processing result: {e}")
                            break
                            
                    elif result.get('type') == 'working':
                        print("⏳ Processing screenshot...")
                        continue
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout {attempt + 1}/5")
                    continue
            
            return False
            
    except Exception as e:
        print(f"❌ Screenshot error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(take_screenshot())
    print(f"\n🎯 SCREENSHOT RESULT: {'SUCCESS' if result else 'FAILED'}")