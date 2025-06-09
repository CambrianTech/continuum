#!/usr/bin/env python3
"""
Connect both Python debugger and browser client to the universal bus
"""
import asyncio
import websockets
import json
import base64
import time

async def connect_both_clients_to_bus():
    print("🚌 CONNECTING BOTH CLIENTS TO UNIVERSAL BUS")
    print("=" * 60)
    print("Python debugger + Browser client → Universal Bus Command System")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Python debugger connected to Continuum bus")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Step 1: Ensure browser client is connected and get actual version
            print("\n🔍 STEP 1: Verifying browser client bus connection")
            
            version_check_js = """
            console.log("🚌 BUS: Browser client checking connection and version");
            
            // Check if we're connected to the bus system
            if (typeof window.ws !== 'undefined' && window.ws.readyState === WebSocket.OPEN) {
                console.log("✅ BUS: Browser client connected to bus");
            } else {
                console.log("❌ BUS: Browser client NOT connected to bus");
            }
            
            // Get actual version from page elements
            const versionElement = document.querySelector('.version-badge, [class*="version"], .version');
            let actualVersion = "unknown";
            
            if (versionElement) {
                actualVersion = versionElement.textContent.trim();
                console.log("✅ BUS: Found version element:", actualVersion);
            } else {
                // Try to find version in any text content
                const allElements = document.querySelectorAll('*');
                for (let el of allElements) {
                    const text = el.textContent;
                    if (text && text.match(/v?\\d+\\.\\d+\\.\\d+/)) {
                        actualVersion = text.trim();
                        console.log("✅ BUS: Found version in text:", actualVersion);
                        break;
                    }
                }
            }
            
            return {
                busConnected: typeof window.ws !== 'undefined' && window.ws.readyState === WebSocket.OPEN,
                version: actualVersion,
                timestamp: Date.now()
            };
            """
            
            encoded_version = base64.b64encode(version_check_js.encode()).decode()
            version_command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_version}'
            }
            
            await websocket.send(json.dumps(version_command))
            
            # Get browser connection status and version
            browser_version = "v0.2.1983"  # fallback
            browser_bus_connected = False
            
            for attempt in range(3):
                response = await websocket.recv()
                result = json.loads(response)
                
                if result.get('type') == 'result':
                    try:
                        bus_result = result.get('data', {}).get('result', {}).get('result', {})
                        browser_response = bus_result.get('browserResponse', {})
                        return_value = browser_response.get('result')
                        
                        if return_value:
                            if isinstance(return_value, str):
                                version_data = json.loads(return_value)
                            else:
                                version_data = return_value
                            
                            browser_bus_connected = version_data.get('busConnected', False)
                            browser_version = version_data.get('version', browser_version)
                            
                            print(f"🚌 Browser client bus connection: {'✅ CONNECTED' if browser_bus_connected else '❌ DISCONNECTED'}")
                            print(f"🚌 Browser client version: {browser_version}")
                            break
                            
                    except Exception as e:
                        print(f"Error parsing version data: {e}")
                        
                elif result.get('type') == 'working':
                    continue
            
            # Step 2: Take screenshot using bus coordination
            print(f"\n📸 STEP 2: Taking screenshot via bus with version {browser_version}")
            
            screenshot_js = f"""
            console.log("📸 BUS: Starting coordinated screenshot with version {browser_version}");
            
            // Create a proper test element with the actual version
            const targetElement = document.createElement('div');
            targetElement.style.cssText = `
                width: 200px;
                height: 80px;
                background: linear-gradient(135deg, #0066cc, #004499);
                color: white;
                padding: 15px;
                border-radius: 10px;
                font-family: 'Monaco', 'Consolas', monospace;
                font-size: 14px;
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
                <div style="font-size: 12px; opacity: 0.9;">{browser_version}</div>
                <div style="font-size: 10px; opacity: 0.7;">Bus Connected</div>
            `;
            targetElement.id = 'bus-screenshot-element';
            document.body.appendChild(targetElement);
            
            console.log("📸 BUS: Created version element with", "{browser_version}");
            
            // Remove all canvas elements to prevent createPattern errors
            const allCanvases = document.querySelectorAll('canvas');
            const removedCanvases = [];
            
            allCanvases.forEach((canvas, i) => {{
                console.log("📸 BUS: Removing canvas", i + ":", canvas.width + "x" + canvas.height);
                removedCanvases.push({{
                    element: canvas,
                    parent: canvas.parentNode,
                    nextSibling: canvas.nextSibling
                }});
                canvas.remove();
            }});
            
            console.log("📸 BUS: Removed", removedCanvases.length, "canvas elements");
            
            return html2canvas(targetElement, {{
                allowTaint: true,
                useCORS: true,
                scale: 2,
                backgroundColor: null
            }}).then(function(canvas) {{
                // Restore canvas elements
                removedCanvases.forEach((item, i) => {{
                    if (item.parent) {{
                        if (item.nextSibling) {{
                            item.parent.insertBefore(item.element, item.nextSibling);
                        }} else {{
                            item.parent.appendChild(item.element);
                        }}
                    }}
                }});
                
                console.log("✅ BUS: Screenshot captured successfully!");
                console.log("✅ BUS: Canvas dimensions:", canvas.width + "x" + canvas.height);
                
                const dataURL = canvas.toDataURL('image/png');
                const timestamp = Date.now();
                const filename = `bus_screenshot_{browser_version}_${{timestamp}}.png`;
                
                console.log("✅ BUS: DataURL length:", dataURL.length);
                console.log("✅ BUS: Filename:", filename);
                
                // Send via WebSocket to save to .continuum/screenshots/
                if (window.ws && window.ws.readyState === WebSocket.OPEN) {{
                    console.log("💾 BUS: Saving screenshot via WebSocket bus");
                    window.ws.send(JSON.stringify({{
                        type: 'screenshot_data',
                        filename: filename,
                        dataURL: dataURL,
                        dimensions: {{
                            width: canvas.width,
                            height: canvas.height
                        }},
                        timestamp: timestamp,
                        version: "{browser_version}",
                        source: 'bus_coordination'
                    }}));
                    console.log("💾 BUS: Screenshot save request sent via bus");
                }}
                
                // Clean up test element
                targetElement.remove();
                console.log("📸 BUS: Cleanup complete");
                
                return {{
                    success: true,
                    width: canvas.width,
                    height: canvas.height,
                    dataLength: dataURL.length,
                    filename: filename,
                    version: "{browser_version}",
                    busConnected: true,
                    savedToDirectory: ".continuum/screenshots/",
                    message: "Bus coordinated screenshot - saved via WebSocket"
                }};
                
            }}).catch(function(error) {{
                // Restore canvas elements on error
                removedCanvases.forEach((item, i) => {{
                    if (item.parent) {{
                        if (item.nextSibling) {{
                            item.parent.insertBefore(item.element, item.nextSibling);
                        }} else {{
                            item.parent.appendChild(item.element);
                        }}
                    }}
                }});
                
                targetElement.remove();
                console.error("❌ BUS: Screenshot failed:", error.message);
                
                return {{
                    success: false,
                    error: error.message,
                    version: "{browser_version}",
                    busConnected: true,
                    message: "Bus coordinated screenshot failed"
                }};
            }});
            """
            
            encoded_screenshot = base64.b64encode(screenshot_js.encode()).decode()
            screenshot_command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_screenshot}'
            }
            
            print("📤 Sending bus-coordinated screenshot command...")
            await websocket.send(json.dumps(screenshot_command))
            
            # Wait for screenshot result and file save
            screenshot_filename = None
            screenshot_success = False
            
            for attempt in range(8):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=3)
                    result = json.loads(response)
                    
                    if result.get('type') == 'result':
                        try:
                            bus_result = result.get('data', {}).get('result', {}).get('result', {})
                            browser_response = bus_result.get('browserResponse', {})
                            return_value = browser_response.get('result')
                            
                            if return_value:
                                if isinstance(return_value, str):
                                    screenshot_result = json.loads(return_value)
                                else:
                                    screenshot_result = return_value
                                
                                if screenshot_result.get('success'):
                                    screenshot_success = True
                                    screenshot_filename = screenshot_result.get('filename')
                                    
                                    print(f"\\n🎉 BUS SCREENSHOT SUCCESS!")
                                    print(f"   📏 Dimensions: {screenshot_result.get('width')}x{screenshot_result.get('height')}")
                                    print(f"   💾 Filename: {screenshot_filename}")
                                    print(f"   📂 Directory: {screenshot_result.get('savedToDirectory')}")
                                    print(f"   🚌 Version: {screenshot_result.get('version')}")
                                    print(f"   📝 Message: {screenshot_result.get('message')}")
                                    break
                                else:
                                    print(f"❌ BUS: Screenshot failed: {screenshot_result.get('error')}")
                                    break
                                    
                        except Exception as e:
                            print(f"Error parsing screenshot result: {e}")
                            
                    elif result.get('type') == 'working':
                        continue
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Waiting for bus screenshot result... {attempt + 1}/8")
                    continue
            
            # Step 3: Verify file was saved and read the version from it
            if screenshot_success and screenshot_filename:
                print(f"\\n🔍 STEP 3: Verifying screenshot file save")
                
                # Wait a moment for file to be written
                await asyncio.sleep(1)
                
                import os
                screenshot_path = f".continuum/screenshots/{screenshot_filename}"
                
                if os.path.exists(screenshot_path):
                    file_size = os.path.getsize(screenshot_path)
                    print(f"✅ BUS: Screenshot file saved successfully!")
                    print(f"   📂 Path: {screenshot_path}")
                    print(f"   📊 File size: {file_size} bytes")
                    print(f"   🚌 Ready to read version from image")
                    return screenshot_path, browser_version
                else:
                    print(f"❌ BUS: Screenshot file not found at {screenshot_path}")
                    return None, browser_version
            else:
                print(f"❌ BUS: Screenshot coordination failed")
                return None, browser_version
                
    except Exception as e:
        print(f"❌ Bus connection error: {e}")
        return None, "unknown"

if __name__ == "__main__":
    result = asyncio.run(connect_both_clients_to_bus())
    screenshot_path, version = result if result else (None, "unknown")
    
    print(f"\\n🎯 BUS COORDINATION RESULT:")
    print(f"   Screenshot: {'SUCCESS' if screenshot_path else 'FAILED'}")
    print(f"   Version: {version}")
    if screenshot_path:
        print(f"   File: {screenshot_path}")
        print(f"\\n🔍 Ready to read version from screenshot: {screenshot_path}")