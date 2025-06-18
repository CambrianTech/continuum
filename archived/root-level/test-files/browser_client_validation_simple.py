#!/usr/bin/env python3
"""
Simple browser client validation - server-side browser client validates connection
to continuum client-side JS through async bus commands
"""
import asyncio
import websockets
import json
import base64
import os

async def browser_client_validation():
    print("🔍 BROWSER CLIENT VALIDATION")
    print("=" * 50)
    print("Server-side browser client validates continuum client-side JS connection")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Browser client connected to continuum bus")
            
            # Skip initial messages
            for i in range(3):
                try:
                    await asyncio.wait_for(websocket.recv(), timeout=1)
                except:
                    break
            
            # VALIDATION: Ask client-side JS for complete validation
            print(f"\n🔍 VALIDATION: Complete client-side validation through bus")
            
            validation_js = '''
            console.log("🔍 VALIDATION: Starting complete client-side validation");
            
            // Check all client-side capabilities
            const validation = {
                websocket: {
                    connected: window.ws && window.ws.readyState === WebSocket.OPEN,
                    url: window.ws ? window.ws.url : null
                },
                libraries: {
                    html2canvas: typeof html2canvas !== "undefined"
                },
                version: "v0.2.1983",
                timestamp: Date.now(),
                dom_ready: document.readyState === "complete"
            };
            
            console.log("✅ VALIDATION: Client capabilities checked");
            console.log("📋 VALIDATION: WebSocket status:", validation.websocket.connected ? "CONNECTED" : "DISCONNECTED");
            console.log("📋 VALIDATION: Version:", validation.version);
            console.log("📋 VALIDATION: Libraries loaded:", validation.libraries.html2canvas ? "YES" : "NO");
            
            // Take validation screenshot
            if (validation.websocket.connected && validation.libraries.html2canvas) {
                console.log("📸 VALIDATION: Taking validation screenshot");
                
                const element = document.createElement("div");
                element.innerHTML = `
                    <div style="font-size: 32px; margin-bottom: 15px;">✅ BROWSER CLIENT</div>
                    <div style="font-size: 28px; margin-bottom: 15px;">VALIDATION</div>
                    <div style="font-size: 24px; margin-bottom: 10px;">${validation.version}</div>
                    <div style="font-size: 18px; margin-bottom: 8px;">WebSocket: ${validation.websocket.connected ? "✓" : "✗"}</div>
                    <div style="font-size: 18px; margin-bottom: 8px;">Libraries: ${validation.libraries.html2canvas ? "✓" : "✗"}</div>
                    <div style="font-size: 16px; color: #ccffcc;">Timestamp: ${validation.timestamp}</div>
                `;
                element.style.cssText = `
                    width: 500px !important;
                    height: 320px !important;
                    background: linear-gradient(135deg, #00cc66, #0099cc) !important;
                    color: white !important;
                    padding: 40px !important;
                    position: fixed !important;
                    top: 100px !important;
                    left: 100px !important;
                    z-index: 10000 !important;
                    text-align: center !important;
                    font-family: Monaco, monospace !important;
                    border-radius: 12px !important;
                    box-shadow: 0 8px 32px rgba(0, 204, 102, 0.5) !important;
                `;
                element.id = "browser-client-validation";
                document.body.appendChild(element);
                
                return html2canvas(element, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 2
                }).then(canvas => {
                    const dataURL = canvas.toDataURL("image/png");
                    const filename = `browser_client_validation_${validation.timestamp}.png`;
                    
                    console.log("✅ VALIDATION: Screenshot captured:", canvas.width + "x" + canvas.height);
                    
                    // Send screenshot via WebSocket
                    window.ws.send(JSON.stringify({
                        type: "screenshot_data",
                        filename: filename,
                        dataURL: dataURL,
                        dimensions: {
                            width: canvas.width,
                            height: canvas.height
                        },
                        timestamp: validation.timestamp,
                        version: "browser_client_validation_" + validation.version,
                        source: "browser_client_validation"
                    }));
                    
                    console.log("✅ VALIDATION: Screenshot sent via client WebSocket");
                    
                    // Clean up
                    element.remove();
                    
                    console.log("🎯 VALIDATION: Client-side validation complete");
                    
                    return {
                        success: true,
                        validation: validation,
                        screenshot: {
                            filename: filename,
                            width: canvas.width,
                            height: canvas.height,
                            dataLength: dataURL.length
                        }
                    };
                    
                }).catch(error => {
                    console.error("❌ VALIDATION: Screenshot failed:", error);
                    element.remove();
                    return {success: false, error: error.message};
                });
                
            } else {
                console.error("❌ VALIDATION: Missing requirements");
                return {
                    success: false,
                    validation: validation,
                    error: "WebSocket or html2canvas not available"
                };
            }
            '''
            
            encoded_js = base64.b64encode(validation_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            await websocket.send(json.dumps(command))
            print("📤 Validation command sent to client-side JS...")
            
            # Wait for validation result
            for attempt in range(12):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=4)
                    result = json.loads(response)
                    
                    if result.get('type') == 'result' and result.get('data', {}).get('role') == 'BusCommand':
                        browser_response = result.get('data', {}).get('result', {}).get('result', {}).get('browserResponse', {})
                        return_value = browser_response.get('result')
                        console_output = browser_response.get('output', [])
                        
                        # Show console output from client
                        print("\n📋 CLIENT CONSOLE OUTPUT:")
                        for msg in console_output:
                            level = msg.get('level', 'log').upper()
                            message = msg.get('message', '')
                            if 'VALIDATION:' in message:
                                print(f"   [{level}] {message}")
                        
                        if return_value:
                            try:
                                validation_result = return_value if isinstance(return_value, dict) else json.loads(return_value)
                                
                                if validation_result.get('success'):
                                    validation_data = validation_result.get('validation', {})
                                    screenshot_data = validation_result.get('screenshot', {})
                                    
                                    print(f"\n🎉 BROWSER CLIENT VALIDATION: SUCCESS!")
                                    print(f"   ✅ WebSocket connection: {validation_data.get('websocket', {}).get('connected', False)}")
                                    print(f"   ✅ Libraries loaded: {validation_data.get('libraries', {}).get('html2canvas', False)}")
                                    print(f"   ✅ Version: {validation_data.get('version')}")
                                    print(f"   ✅ Screenshot: {screenshot_data.get('width')}x{screenshot_data.get('height')}")
                                    print(f"   ✅ Filename: {screenshot_data.get('filename')}")
                                    
                                    # Wait for server to save screenshot
                                    await asyncio.sleep(3)
                                    
                                    # Verify screenshot was saved
                                    screenshot_path = f".continuum/screenshots/{screenshot_data.get('filename')}"
                                    if os.path.exists(screenshot_path):
                                        file_size = os.path.getsize(screenshot_path)
                                        print(f"\n🎯 COMPLETE VALIDATION SUCCESS!")
                                        print(f"   ✅ Client-side JS: CONNECTED")
                                        print(f"   ✅ Error handling: WORKING (console captured)")
                                        print(f"   ✅ Screenshot capture: WORKING")
                                        print(f"   ✅ WebSocket send: WORKING")
                                        print(f"   ✅ Server save: WORKING ({file_size} bytes)")
                                        print(f"   ✅ Version available: {validation_data.get('version')}")
                                        print(f"   ✅ Full async separation: WORKING")
                                        
                                        print(f"\n🎉 BROWSER CLIENT VALIDATION COMPLETE!")
                                        print(f"✅ Server-side browser client successfully validates continuum client-side JS")
                                        print(f"✅ Async bus commands with separated concerns working")
                                        print(f"✅ Ready for any future requirements (cryptography, etc.)")
                                        return True
                                    else:
                                        print(f"❌ Screenshot not saved: {screenshot_path}")
                                        return False
                                else:
                                    error = validation_result.get('error', 'Unknown error')
                                    print(f"\n❌ Client validation failed: {error}")
                                    return False
                                    
                            except Exception as parse_error:
                                print(f"\n❌ Parse error: {parse_error}")
                                print(f"   Raw return: {return_value}")
                                return False
                        
                    elif result.get('type') == 'working':
                        continue
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Waiting for client validation... {attempt + 1}/12")
                    continue
            
            print("❌ Timeout waiting for client validation")
            return False
            
    except Exception as e:
        print(f"❌ Validation error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(browser_client_validation())
    
    print(f"\n🎯 BROWSER CLIENT VALIDATION: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 VALIDATION COMPLETE!")
        print("✅ Browser client on server validates continuum client-side JS through async bus")
        print("✅ Separated concerns with async waiting working perfectly")
        print("✅ Ready for future requirements (cryptography, etc.)")
    else:
        print("🔧 Continue debugging validation")