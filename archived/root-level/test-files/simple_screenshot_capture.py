#!/usr/bin/env python3
"""
Simple screenshot capture using working Python debugger connection
"""
import asyncio
import websockets
import json
import base64

async def capture_screenshot():
    print("📸 SIMPLE SCREENSHOT CAPTURE")
    print("=" * 50)
    print("Using working Python debugger to capture screenshot")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Python debugger connected")
            
            # Skip initial messages  
            await websocket.recv()
            await websocket.recv()
            
            # Wait for any greetings to clear
            await asyncio.sleep(1)
            
            # Capture screenshot in browser and return the data
            screenshot_js = '''
            console.log("📸 CAPTURE: Starting screenshot capture");
            
            // Create version element
            const testElement = document.createElement('div');
            testElement.style.cssText = 'width:300px;height:120px;background:linear-gradient(135deg,#0066cc,#004499);color:white;padding:20px;border-radius:12px;font-family:Monaco,monospace;position:fixed;top:50px;left:50px;z-index:10000;text-align:center;font-weight:bold;box-shadow:0 4px 15px rgba(0,102,204,0.3);';
            testElement.innerHTML = '<div style="font-size:18px;">✅ MILESTONE 6</div><div style="font-size:16px;margin:8px 0;">v0.2.1983</div><div style="font-size:12px;">Screenshot Complete</div>';
            testElement.id = 'milestone-screenshot-element';
            document.body.appendChild(testElement);
            
            console.log("📸 CAPTURE: Created test element");
            
            if (typeof html2canvas !== 'undefined') {
                return html2canvas(testElement, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 2,
                    backgroundColor: null
                }).then(function(canvas) {
                    console.log("✅ CAPTURE: Screenshot captured", canvas.width + "x" + canvas.height);
                    
                    const dataURL = canvas.toDataURL('image/png');
                    const timestamp = Date.now();
                    
                    console.log("📸 CAPTURE: DataURL length:", dataURL.length);
                    
                    // Clean up
                    testElement.remove();
                    console.log("📸 CAPTURE: Cleanup completed");
                    
                    // Return the screenshot data to Python
                    return JSON.stringify({
                        success: true,
                        width: canvas.width,
                        height: canvas.height,
                        dataURL: dataURL,
                        timestamp: timestamp,
                        version: "v0.2.1983"
                    });
                    
                }).catch(function(error) {
                    console.error("❌ CAPTURE: Screenshot failed:", error.message);
                    testElement.remove();
                    
                    return JSON.stringify({
                        success: false,
                        error: error.message
                    });
                });
            } else {
                console.error("❌ CAPTURE: html2canvas not available");
                testElement.remove();
                return JSON.stringify({success: false, error: "html2canvas not available"});
            }
            '''
            
            encoded_js = base64.b64encode(screenshot_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print("📤 Sending screenshot capture command...")
            await websocket.send(json.dumps(command))
            
            # Wait for screenshot result
            for attempt in range(8):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=5)
                    result = json.loads(response)
                    
                    if result.get('type') == 'working':
                        continue
                        
                    elif result.get('type') == 'result':
                        data = result.get('data', {})
                        
                        if data.get('role') == 'BusCommand':
                            browser_response = data.get('result', {}).get('result', {}).get('browserResponse', {})
                            return_value = browser_response.get('result')
                            
                            if return_value:
                                try:
                                    screenshot_data = json.loads(return_value)
                                    
                                    if screenshot_data.get('success'):
                                        print(f"✅ SCREENSHOT CAPTURED!")
                                        print(f"   Dimensions: {screenshot_data.get('width')}x{screenshot_data.get('height')}")
                                        print(f"   Version: {screenshot_data.get('version')}")
                                        print(f"   DataURL length: {screenshot_data.get('dataURL', '')[:50]}...")
                                        
                                        # Now save it using Python debugger connection
                                        dataURL = screenshot_data.get('dataURL')
                                        timestamp = screenshot_data.get('timestamp')
                                        filename = f"milestone_6_complete_{timestamp}.png"
                                        
                                        # Send screenshot_data via debugger connection
                                        save_message = {
                                            "type": "screenshot_data",
                                            "filename": filename,
                                            "dataURL": dataURL,
                                            "dimensions": {
                                                "width": screenshot_data.get('width'),
                                                "height": screenshot_data.get('height')
                                            },
                                            "timestamp": timestamp,
                                            "version": screenshot_data.get('version'),
                                            "source": "python_debugger_milestone_6"
                                        }
                                        
                                        print(f"💾 Saving screenshot: {filename}")
                                        await websocket.send(json.dumps(save_message))
                                        
                                        # Wait for save and verify
                                        await asyncio.sleep(2)
                                        
                                        import os
                                        file_path = f".continuum/screenshots/{filename}"
                                        
                                        if os.path.exists(file_path):
                                            file_size = os.path.getsize(file_path)
                                            print(f"🎉 MILESTONE 6 COMPLETE!")
                                            print(f"   ✅ Screenshot captured in browser")
                                            print(f"   ✅ Data transferred via bus commands")
                                            print(f"   ✅ File saved by server: {file_path}")
                                            print(f"   📊 File size: {file_size} bytes")
                                            return True
                                        else:
                                            print(f"❌ File not saved: {file_path}")
                                            return False
                                    else:
                                        print(f"❌ Screenshot failed: {screenshot_data.get('error')}")
                                        return False
                                        
                                except Exception as e:
                                    print(f"❌ Error parsing screenshot data: {e}")
                                    return False
                        
                        elif data.get('task') == 'user_connection_greeting':
                            continue
                            
                except asyncio.TimeoutError:
                    print(f"⏰ Waiting for screenshot... {attempt + 1}/8")
                    continue
            
            return False
            
    except Exception as e:
        print(f"❌ Error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(capture_screenshot())
    
    print(f"\n🎯 SIMPLE SCREENSHOT CAPTURE: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 MILESTONE 6: COMPLETE!")
        print("✅ Full async screenshot chain working!")
        print("✅ Browser → Bus Commands → Python Debugger → Server → File")
    else:
        print("🔧 Need to debug screenshot capture")