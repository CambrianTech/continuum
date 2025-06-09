#!/usr/bin/env python3
"""
Fix browser WebSocket connection and complete MILESTONE 6 using working Python debugger
"""
import asyncio
import websockets
import json
import base64
import os

async def fix_browser_connection_and_m6():
    print("🔗 FIX BROWSER CONNECTION & COMPLETE M6")
    print("=" * 60)
    print("Using working Python debugger to establish browser WebSocket and complete screenshot")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Python debugger connected")
            
            # Consume all initial messages like the working example
            initial_messages = []
            for i in range(5):
                try:
                    message = await asyncio.wait_for(websocket.recv(), timeout=2)
                    result = json.loads(message)
                    initial_messages.append(result)
                    print(f"📥 Initial message {i+1}: {result.get('type')}")
                except asyncio.TimeoutError:
                    print(f"⏰ No more initial messages after {i+1}")
                    break
            
            print(f"\n✅ Consumed {len(initial_messages)} initial messages")
            
            # STEP 1: Establish browser WebSocket connection (fix window.ws)
            print(f"\n🔗 STEP 1: Establishing browser WebSocket connection...")
            
            connection_js = '''
            console.log("🔗 BROWSER: Checking WebSocket connection status");
            
            // Check if WebSocket already exists and is connected
            if (typeof window.ws !== 'undefined' && window.ws.readyState === WebSocket.OPEN) {
                console.log("✅ BROWSER: WebSocket already connected");
                return "WS_ALREADY_CONNECTED";
            }
            
            console.log("🔗 BROWSER: Establishing WebSocket connection...");
            
            // Create WebSocket connection
            const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${wsProtocol}//${window.location.host}`;
            
            console.log("🔗 BROWSER: Connecting to:", wsUrl);
            
            try {
                window.ws = new WebSocket(wsUrl);
                
                return new Promise((resolve, reject) => {
                    window.ws.onopen = function(event) {
                        console.log("✅ BROWSER: WebSocket connected successfully!");
                        console.log("✅ BROWSER: ReadyState:", window.ws.readyState);
                        
                        // Send tab registration
                        const tabInfo = {
                            type: 'tabRegister',
                            tabId: `debugger_tab_${Date.now()}`,
                            version: '0.2.1983',
                            url: window.location.href,
                            timestamp: new Date().toISOString()
                        };
                        
                        window.ws.send(JSON.stringify(tabInfo));
                        console.log("✅ BROWSER: Tab registration sent");
                        
                        resolve("WS_CONNECTION_SUCCESS");
                    };
                    
                    window.ws.onerror = function(error) {
                        console.error("❌ BROWSER: WebSocket error:", error);
                        reject("WS_CONNECTION_ERROR");
                    };
                    
                    window.ws.onclose = function(event) {
                        console.log("🔗 BROWSER: WebSocket closed:", event.code);
                    };
                    
                    // Timeout after 5 seconds
                    setTimeout(() => {
                        if (!window.ws || window.ws.readyState !== WebSocket.OPEN) {
                            console.error("❌ BROWSER: Connection timeout");
                            reject("WS_CONNECTION_TIMEOUT");
                        }
                    }, 5000);
                });
                
            } catch (error) {
                console.error("❌ BROWSER: Failed to create WebSocket:", error);
                return "WS_CREATE_ERROR";
            }
            '''
            
            # Send connection establishment command
            encoded_js = base64.b64encode(connection_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            await websocket.send(json.dumps(command))
            print("📤 Connection command sent...")
            
            # Wait for connection result
            connection_success = False
            for attempt in range(10):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=3)
                    result = json.loads(response)
                    
                    if result.get('type') == 'working':
                        continue
                        
                    elif result.get('type') == 'result':
                        data = result.get('data', {})
                        
                        if data.get('role') == 'BusCommand':
                            browser_response = data.get('result', {}).get('result', {}).get('browserResponse', {})
                            return_value = browser_response.get('result')
                            console_output = browser_response.get('output', [])
                            
                            # Check for success indicators
                            ws_connected = any('WebSocket connected successfully' in msg.get('message', '') for msg in console_output)
                            ws_already = any('WebSocket already connected' in msg.get('message', '') for msg in console_output)
                            
                            if ws_connected or ws_already or return_value in ['WS_CONNECTION_SUCCESS', 'WS_ALREADY_CONNECTED']:
                                print("✅ STEP 1: Browser WebSocket connection established!")
                                connection_success = True
                                break
                            else:
                                print(f"❌ STEP 1: Connection failed - {return_value}")
                                return False
                        
                        elif data.get('task') == 'user_connection_greeting':
                            continue
                            
                except asyncio.TimeoutError:
                    print(f"⏰ Waiting for connection... {attempt + 1}/10")
                    continue
            
            if not connection_success:
                print("❌ STEP 1: Failed to establish browser WebSocket connection")
                return False
            
            # STEP 2: Wait for connection to stabilize
            print(f"\n⏳ STEP 2: Stabilizing connection...")
            await asyncio.sleep(2)
            
            # STEP 3: Complete MILESTONE 6 - Screenshot with WebSocket send
            print(f"\n📸 STEP 3: MILESTONE 6 - Screenshot with WebSocket send...")
            
            screenshot_js = '''
            console.log("📸 M6: Starting screenshot with connected WebSocket");
            
            // Verify WebSocket is connected
            if (!window.ws || window.ws.readyState !== WebSocket.OPEN) {
                console.error("❌ M6: WebSocket not connected for screenshot");
                return JSON.stringify({success: false, error: "WebSocket not connected"});
            }
            
            console.log("✅ M6: WebSocket confirmed connected, proceeding with screenshot");
            
            // Create milestone completion element
            const milestoneElement = document.createElement('div');
            milestoneElement.style.cssText = `
                width: 320px;
                height: 160px;
                background: linear-gradient(135deg, #00cc66, #0099cc);
                color: white;
                padding: 20px;
                border-radius: 12px;
                font-family: 'Monaco', 'Consolas', monospace;
                position: fixed;
                top: 100px;
                left: 100px;
                z-index: 10000;
                text-align: center;
                font-weight: bold;
                box-shadow: 0 8px 32px rgba(0, 204, 102, 0.3);
                border: 2px solid rgba(255, 255, 255, 0.2);
            `;
            milestoneElement.innerHTML = `
                <div style="font-size: 24px; margin-bottom: 8px;">✅ MILESTONE 6</div>
                <div style="font-size: 18px; margin-bottom: 8px;">COMPLETE</div>
                <div style="font-size: 14px; margin-bottom: 8px;">v0.2.1983</div>
                <div style="font-size: 12px; color: #ccffcc;">WebSocket Connected ✓</div>
                <div style="font-size: 12px; color: #ccffcc;">Screenshot → Bus → Server ✓</div>
            `;
            milestoneElement.id = 'milestone-6-complete';
            document.body.appendChild(milestoneElement);
            
            console.log("📸 M6: Created milestone completion element");
            
            if (typeof html2canvas !== 'undefined') {
                return html2canvas(milestoneElement, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 2,
                    backgroundColor: null
                }).then(function(canvas) {
                    const dataURL = canvas.toDataURL('image/png');
                    const timestamp = Date.now();
                    const filename = `milestone_6_complete_${timestamp}.png`;
                    
                    console.log("✅ M6: Screenshot captured:", canvas.width + "x" + canvas.height);
                    console.log("📸 M6: DataURL length:", dataURL.length);
                    console.log("📸 M6: Filename:", filename);
                    
                    // Send screenshot via connected WebSocket
                    const message = {
                        type: 'screenshot_data',
                        filename: filename,
                        dataURL: dataURL,
                        dimensions: {
                            width: canvas.width,
                            height: canvas.height
                        },
                        timestamp: timestamp,
                        version: 'milestone_6_v0.2.1983',
                        source: 'python_debugger_milestone_6'
                    };
                    
                    window.ws.send(JSON.stringify(message));
                    console.log("✅ M6: Screenshot sent via WebSocket successfully!");
                    
                    // Clean up element
                    milestoneElement.remove();
                    console.log("📸 M6: Element cleanup completed");
                    
                    return JSON.stringify({
                        success: true,
                        filename: filename,
                        width: canvas.width,
                        height: canvas.height,
                        dataLength: dataURL.length,
                        message: "MILESTONE 6 COMPLETE"
                    });
                    
                }).catch(function(error) {
                    console.error("❌ M6: Screenshot failed:", error.message);
                    milestoneElement.remove();
                    return JSON.stringify({success: false, error: error.message});
                });
            } else {
                console.error("❌ M6: html2canvas not available");
                milestoneElement.remove();
                return JSON.stringify({success: false, error: "html2canvas not available"});
            }
            '''
            
            # Send screenshot command
            encoded_js = base64.b64encode(screenshot_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system', 
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            await websocket.send(json.dumps(command))
            print("📤 Screenshot command sent...")
            
            # Wait for screenshot result
            for attempt in range(12):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=5)
                    result = json.loads(response)
                    
                    if result.get('type') == 'working':
                        continue
                        
                    elif result.get('type') == 'result':
                        data = result.get('data', {})
                        
                        if data.get('role') == 'BusCommand':
                            browser_response = data.get('result', {}).get('result', {}).get('result', {}).get('browserResponse', {})
                            return_value = browser_response.get('result')
                            
                            if return_value:
                                try:
                                    screenshot_result = json.loads(return_value)
                                    
                                    if screenshot_result.get('success'):
                                        filename = screenshot_result.get('filename')
                                        width = screenshot_result.get('width')
                                        height = screenshot_result.get('height')
                                        
                                        print(f"✅ STEP 3: Screenshot captured successfully!")
                                        print(f"   📏 Dimensions: {width}x{height}")
                                        print(f"   📁 Filename: {filename}")
                                        
                                        # Wait for file to be saved by server
                                        await asyncio.sleep(3)
                                        
                                        # Verify file was saved
                                        screenshot_path = f".continuum/screenshots/{filename}"
                                        if os.path.exists(screenshot_path):
                                            file_size = os.path.getsize(screenshot_path)
                                            print(f"✅ STEP 3: File saved successfully!")
                                            print(f"   📁 Path: {screenshot_path}")
                                            print(f"   📊 Size: {file_size} bytes")
                                            
                                            print(f"\n🎉 MILESTONE 6: COMPLETE!")
                                            print(f"✅ Browser WebSocket connection: WORKING")
                                            print(f"✅ Screenshot capture: WORKING ({width}x{height})")
                                            print(f"✅ WebSocket send: WORKING")
                                            print(f"✅ Server save: WORKING ({file_size} bytes)")
                                            print(f"✅ Full async chain: WORKING")
                                            
                                            return True
                                        else:
                                            print(f"❌ STEP 3: File not saved to {screenshot_path}")
                                            return False
                                    else:
                                        print(f"❌ STEP 3: Screenshot failed - {screenshot_result.get('error')}")
                                        return False
                                        
                                except Exception as parse_error:
                                    print(f"❌ STEP 3: Parse error - {parse_error}")
                                    return False
                        
                        elif data.get('task') == 'user_connection_greeting':
                            continue
                            
                except asyncio.TimeoutError:
                    print(f"⏰ Waiting for screenshot... {attempt + 1}/12")
                    continue
            
            print("❌ STEP 3: Screenshot validation timeout")
            return False
            
    except Exception as e:
        print(f"❌ Error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(fix_browser_connection_and_m6())
    
    print(f"\n🎯 BROWSER CONNECTION & M6: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 MILESTONE 6 VALIDATION COMPLETE!")
        print("✅ Browser WebSocket established (window.ws working)")
        print("✅ Screenshot capture via WebSocket working")
        print("✅ Full async promise chain working")
        print("✅ Ready for remaining milestone validation!")
    else:
        print("🔧 Continue debugging browser connection issues")