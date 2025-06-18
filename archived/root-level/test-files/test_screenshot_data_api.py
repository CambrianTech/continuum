#!/usr/bin/env python3
"""
Test screenshot_data API directly using debugger connection
"""
import asyncio
import websockets
import json
import base64
import time

async def test_screenshot_data_api():
    print("🔍 TESTING SCREENSHOT_DATA API")
    print("=" * 50)
    print("Using debugger connection to test server's screenshot_data reception")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Debugger connected to Continuum")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Create a test base64 image (small PNG)
            # This is a tiny 1x1 red pixel PNG in base64
            test_image_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChAGA4zNKmwAAAABJRU5ErkJggg=="
            test_dataURL = f"data:image/png;base64,{test_image_base64}"
            
            timestamp = int(time.time() * 1000)
            filename = f"debugger_test_screenshot_{timestamp}.png"
            
            # Test 1: Send screenshot_data message directly from debugger
            print(f"\n📤 TEST 1: Sending screenshot_data from debugger")
            
            screenshot_message = {
                "type": "screenshot_data",
                "filename": filename,
                "dataURL": test_dataURL,
                "dimensions": {
                    "width": 1,
                    "height": 1
                },
                "timestamp": timestamp,
                "version": "debugger_test",
                "source": "python_debugger"
            }
            
            print(f"   📝 Filename: {filename}")
            print(f"   📏 Dimensions: 1x1")
            print(f"   💾 DataURL length: {len(test_dataURL)}")
            
            await websocket.send(json.dumps(screenshot_message))
            print(f"   ✅ Message sent to server")
            
            # Wait a moment for server processing
            await asyncio.sleep(1)
            
            # Check if file was created
            import os
            screenshot_path = f".continuum/screenshots/{filename}"
            
            if os.path.exists(screenshot_path):
                file_size = os.path.getsize(screenshot_path)
                print(f"   ✅ SUCCESS: File saved at {screenshot_path}")
                print(f"   📊 File size: {file_size} bytes")
                
                # Verify it's a valid PNG
                with open(screenshot_path, 'rb') as f:
                    header = f.read(8)
                    is_png = header == b'\x89PNG\r\n\x1a\n'
                    print(f"   🖼️  Valid PNG: {'✅ YES' if is_png else '❌ NO'}")
                
                return True
            else:
                print(f"   ❌ FAILED: File not saved at {screenshot_path}")
                
                # Check if directory exists
                screenshots_dir = ".continuum/screenshots"
                if os.path.exists(screenshots_dir):
                    files = os.listdir(screenshots_dir)
                    print(f"   📂 Screenshots directory exists, contains {len(files)} files:")
                    for f in files[:5]:  # Show first 5 files
                        print(f"      - {f}")
                else:
                    print(f"   📂 Screenshots directory does not exist")
                
                return False
                
    except Exception as e:
        print(f"❌ Test error: {e}")
        return False

async def test_browser_screenshot_flow():
    print(f"\n🌐 TEST 2: Browser screenshot flow end-to-end")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected for browser flow test")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Execute browser-side screenshot that should send screenshot_data
            browser_js = '''
            console.log("🔍 FLOW TEST: Starting browser screenshot flow");
            
            // Create test element
            const testElement = document.createElement('div');
            testElement.style.cssText = 'width:100px;height:50px;background:#00ff00;color:black;padding:10px;border-radius:5px;font-family:monospace;position:fixed;top:100px;left:100px;z-index:10000;text-align:center;font-weight:bold;';
            testElement.innerHTML = 'FLOW TEST';
            testElement.id = 'flow-test-element';
            document.body.appendChild(testElement);
            console.log("🔍 FLOW TEST: Test element created");
            
            if (typeof html2canvas !== 'undefined') {
                console.log("🔍 FLOW TEST: html2canvas available, starting capture...");
                
                html2canvas(testElement, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 1
                }).then(function(canvas) {
                    console.log("✅ FLOW TEST: Screenshot captured", canvas.width + "x" + canvas.height);
                    
                    const dataURL = canvas.toDataURL('image/png');
                    const timestamp = Date.now();
                    const filename = `flow_test_${timestamp}.png`;
                    
                    console.log("🔍 FLOW TEST: Generated filename:", filename);
                    console.log("🔍 FLOW TEST: DataURL length:", dataURL.length);
                    
                    // Send via WebSocket
                    if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                        console.log("📤 FLOW TEST: Sending via WebSocket...");
                        
                        const message = {
                            type: 'screenshot_data',
                            filename: filename,
                            dataURL: dataURL,
                            dimensions: {
                                width: canvas.width,
                                height: canvas.height
                            },
                            timestamp: timestamp,
                            version: "flow_test",
                            source: 'browser_flow_test'
                        };
                        
                        window.ws.send(JSON.stringify(message));
                        console.log("✅ FLOW TEST: WebSocket message sent");
                        
                    } else {
                        console.error("❌ FLOW TEST: WebSocket not ready");
                    }
                    
                    // Clean up
                    testElement.remove();
                    console.log("🔍 FLOW TEST: Cleanup complete");
                    
                }).catch(function(error) {
                    console.error("❌ FLOW TEST: Screenshot failed:", error.message);
                    testElement.remove();
                });
                
            } else {
                console.error("❌ FLOW TEST: html2canvas not available");
                testElement.remove();
            }
            
            console.log("🔍 FLOW TEST: Browser flow initiated");
            return "FLOW_TEST_STARTED";
            '''
            
            encoded_js = base64.b64encode(browser_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print("📤 Sending browser screenshot flow command...")
            await websocket.send(json.dumps(command))
            
            # Wait for execution and check for file creation
            await asyncio.sleep(3)
            
            # Check for any new flow_test files
            import os
            import glob
            
            flow_files = glob.glob(".continuum/screenshots/flow_test_*.png")
            if flow_files:
                latest_file = max(flow_files, key=os.path.getctime)
                file_size = os.path.getsize(latest_file)
                print(f"✅ FLOW TEST SUCCESS: {latest_file} ({file_size} bytes)")
                return True
            else:
                print("❌ FLOW TEST FAILED: No flow_test files created")
                return False
                
    except Exception as e:
        print(f"❌ Flow test error: {e}")
        return False

if __name__ == "__main__":
    async def main():
        # Test 1: Direct API test
        api_result = await test_screenshot_data_api()
        
        # Test 2: Browser flow test
        flow_result = await test_browser_screenshot_flow()
        
        print(f"\n🎯 TEST RESULTS:")
        print(f"   Direct API test: {'✅ PASS' if api_result else '❌ FAIL'}")
        print(f"   Browser flow test: {'✅ PASS' if flow_result else '❌ FAIL'}")
        
        if api_result and flow_result:
            print(f"\n🎉 SCREENSHOT_DATA API: FULLY WORKING!")
        elif api_result:
            print(f"\n⚠️  API works but browser flow has issues")
        elif flow_result:
            print(f"\n⚠️  Browser flow works but direct API has issues")
        else:
            print(f"\n❌ SCREENSHOT_DATA API: NOT WORKING")
    
    asyncio.run(main())