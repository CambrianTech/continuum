#!/usr/bin/env python3
"""
Capture real screenshot using the working Python debugger
"""
import asyncio
import websockets
import json
import base64

async def capture_real_screenshot():
    print("📸 CAPTURING REAL SCREENSHOT")
    print("=" * 40)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Debugger connected")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            await websocket.recv()  # Skip greeting too
            
            # Capture the actual browser content
            real_screenshot_js = '''
            console.log("📸 REAL SCREENSHOT: Starting actual browser capture");
            
            // Take screenshot of the entire page content, not a test element
            if (typeof html2canvas !== 'undefined') {
                console.log("📸 REAL SCREENSHOT: html2canvas available, capturing whole page");
                
                return html2canvas(document.body, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 1,
                    width: window.innerWidth,
                    height: window.innerHeight,
                    backgroundColor: '#ffffff'
                }).then(function(canvas) {
                    console.log("✅ REAL SCREENSHOT: Captured", canvas.width + "x" + canvas.height);
                    
                    const dataURL = canvas.toDataURL('image/png');
                    const timestamp = Date.now();
                    
                    console.log("📸 REAL SCREENSHOT: DataURL length:", dataURL.length);
                    console.log("📸 REAL SCREENSHOT: Canvas size:", canvas.width, "x", canvas.height);
                    
                    return JSON.stringify({
                        success: true,
                        width: canvas.width,
                        height: canvas.height,
                        dataURL: dataURL,
                        timestamp: timestamp,
                        source: "real_browser_screenshot"
                    });
                    
                }).catch(function(error) {
                    console.error("❌ REAL SCREENSHOT: Failed:", error.message);
                    return JSON.stringify({
                        success: false,
                        error: error.message
                    });
                });
            } else {
                console.error("❌ REAL SCREENSHOT: html2canvas not available");
                return JSON.stringify({success: false, error: "html2canvas not available"});
            }
            '''
            
            encoded_js = base64.b64encode(real_screenshot_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print("📤 Sending real screenshot command...")
            await websocket.send(json.dumps(command))
            
            # Wait for the real screenshot
            for attempt in range(10):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=10)
                    result = json.loads(response)
                    
                    if result.get('type') == 'working':
                        print("⏳ Processing screenshot...")
                        continue
                        
                    elif result.get('type') == 'result':
                        data = result.get('data', {})
                        
                        if data.get('role') == 'BusCommand':
                            browser_response = data.get('result', {}).get('result', {}).get('browserResponse', {})
                            return_value = browser_response.get('result')
                            
                            if return_value:
                                try:
                                    screenshot_result = json.loads(return_value)
                                    
                                    if screenshot_result.get('success'):
                                        width = screenshot_result.get('width')
                                        height = screenshot_result.get('height')
                                        dataURL = screenshot_result.get('dataURL')
                                        timestamp = screenshot_result.get('timestamp')
                                        
                                        print(f"✅ REAL SCREENSHOT CAPTURED!")
                                        print(f"   📏 Size: {width}x{height}")
                                        print(f"   💾 Data length: {len(dataURL) if dataURL else 0} characters")
                                        
                                        if width > 100 and height > 100 and dataURL:
                                            # Save the real screenshot
                                            filename = f"real_screenshot_{timestamp}.png"
                                            
                                            save_message = {
                                                "type": "screenshot_data",
                                                "filename": filename,
                                                "dataURL": dataURL,
                                                "dimensions": {"width": width, "height": height},
                                                "timestamp": timestamp,
                                                "version": "real_v0.2.1983",
                                                "source": "python_debugger_real"
                                            }
                                            
                                            print(f"💾 Saving real screenshot: {filename}")
                                            await websocket.send(json.dumps(save_message))
                                            await asyncio.sleep(2)
                                            
                                            # Check file
                                            import os
                                            file_path = f".continuum/screenshots/{filename}"
                                            
                                            if os.path.exists(file_path):
                                                file_size = os.path.getsize(file_path)
                                                print(f"🎉 SUCCESS: Real screenshot saved!")
                                                print(f"   📁 File: {file_path}")
                                                print(f"   📊 Size: {file_size} bytes")
                                                print(f"   📸 Dimensions: {width}x{height}")
                                                return True
                                            else:
                                                print(f"❌ File not saved")
                                                return False
                                        else:
                                            print(f"❌ Screenshot too small: {width}x{height}")
                                            return False
                                    else:
                                        print(f"❌ Screenshot failed: {screenshot_result.get('error')}")
                                        return False
                                        
                                except Exception as e:
                                    print(f"❌ Parse error: {e}")
                                    return False
                        
                        elif data.get('task') == 'user_connection_greeting':
                            continue
                            
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout {attempt + 1}/10")
                    continue
            
            return False
            
    except Exception as e:
        print(f"❌ Error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(capture_real_screenshot())
    
    print(f"\n🎯 REAL SCREENSHOT: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 MILESTONE 6: COMPLETE with real screenshot!")
    else:
        print("🔧 Fix screenshot capture size issue")