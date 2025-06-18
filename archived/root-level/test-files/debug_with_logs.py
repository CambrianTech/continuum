#!/usr/bin/env python3
"""
Debug using logs from both browser and server side
"""
import asyncio
import websockets
import json
import base64
import time

async def debug_with_logs():
    print("📋 DEBUGGING WITH LOGS")
    print("=" * 50)
    print("Using console logs and server logs to debug WebSocket connection")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Debugger connected")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Step 1: Log browser WebSocket state in detail
            print(f"\n📋 STEP 1: Detailed WebSocket state logging")
            
            ws_debug_js = '''
            console.log("📋 WS DEBUG: === WebSocket State Analysis ===");
            console.log("📋 WS DEBUG: window.ws exists:", typeof window.ws !== 'undefined');
            
            if (window.ws) {
                console.log("📋 WS DEBUG: WebSocket object found");
                console.log("📋 WS DEBUG: readyState:", window.ws.readyState);
                console.log("📋 WS DEBUG: WebSocket.CONNECTING =", WebSocket.CONNECTING);
                console.log("📋 WS DEBUG: WebSocket.OPEN =", WebSocket.OPEN);
                console.log("📋 WS DEBUG: WebSocket.CLOSING =", WebSocket.CLOSING);
                console.log("📋 WS DEBUG: WebSocket.CLOSED =", WebSocket.CLOSED);
                
                const states = ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED'];
                console.log("📋 WS DEBUG: State name:", states[window.ws.readyState]);
                console.log("📋 WS DEBUG: URL:", window.ws.url);
                console.log("📋 WS DEBUG: Protocol:", window.ws.protocol);
                console.log("📋 WS DEBUG: Extensions:", window.ws.extensions);
                console.log("📋 WS DEBUG: Binary type:", window.ws.binaryType);
                
                // Check buffered amount
                console.log("📋 WS DEBUG: Buffered amount:", window.ws.bufferedAmount);
                
                // Test if we can send a simple message
                if (window.ws.readyState === WebSocket.OPEN) {
                    console.log("📋 WS DEBUG: ✅ WebSocket is OPEN, testing send...");
                    try {
                        window.ws.send(JSON.stringify({
                            type: 'debug_ping',
                            message: 'Testing WebSocket send capability',
                            timestamp: Date.now()
                        }));
                        console.log("📋 WS DEBUG: ✅ Send test successful");
                    } catch (error) {
                        console.error("📋 WS DEBUG: ❌ Send test failed:", error.message);
                    }
                } else {
                    console.log("📋 WS DEBUG: ❌ WebSocket not OPEN, cannot send");
                    
                    // Try to understand why it's not open
                    if (window.ws.readyState === WebSocket.CONNECTING) {
                        console.log("📋 WS DEBUG: Still connecting...");
                    } else if (window.ws.readyState === WebSocket.CLOSING) {
                        console.log("📋 WS DEBUG: Connection is closing");
                    } else if (window.ws.readyState === WebSocket.CLOSED) {
                        console.log("📋 WS DEBUG: Connection is closed");
                    }
                }
                
            } else {
                console.error("📋 WS DEBUG: ❌ No window.ws object found");
                console.log("📋 WS DEBUG: Available globals:", Object.keys(window).filter(k => k.includes('ws') || k.includes('socket')));
            }
            
            console.log("📋 WS DEBUG: === End WebSocket Analysis ===");
            return "WS_DEBUG_COMPLETE";
            '''
            
            encoded_js = base64.b64encode(ws_debug_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print("📤 Sending WebSocket debug logging command...")
            await websocket.send(json.dumps(command))
            
            # Step 2: Wait and capture response to see console logs
            print(f"\n📋 STEP 2: Capturing console logs from browser")
            
            for attempt in range(5):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=3)
                    result = json.loads(response)
                    
                    print(f"\n📨 Response type: {result.get('type')}")
                    
                    if result.get('type') == 'result':
                        # Try to extract console output
                        try:
                            data_section = result.get('data', {})
                            result_section = data_section.get('result', {})
                            inner_result = result_section.get('result', {})
                            
                            if isinstance(inner_result, dict):
                                browser_response = inner_result.get('browserResponse', {})
                                console_output = browser_response.get('output', [])
                                
                                print(f"📋 BROWSER CONSOLE LOGS ({len(console_output)} messages):")
                                for i, msg in enumerate(console_output):
                                    level = msg.get('level', 'log').upper()
                                    message = msg.get('message', '')
                                    print(f"   {i+1}. [{level}] {message}")
                                
                                # Analyze WebSocket state from logs
                                open_msgs = [msg for msg in console_output if 'WebSocket is OPEN' in msg.get('message', '')]
                                closed_msgs = [msg for msg in console_output if 'WebSocket not OPEN' in msg.get('message', '')]
                                send_success = [msg for msg in console_output if 'Send test successful' in msg.get('message', '')]
                                send_failed = [msg for msg in console_output if 'Send test failed' in msg.get('message', '')]
                                
                                print(f"\n🔍 WEBSOCKET ANALYSIS:")
                                print(f"   Connection OPEN: {'✅ YES' if open_msgs else '❌ NO'}")
                                print(f"   Connection CLOSED: {'❌ YES' if closed_msgs else '✅ NO'}")
                                print(f"   Send capability: {'✅ WORKING' if send_success else '❌ FAILED'}")
                                
                                if send_failed:
                                    for msg in send_failed:
                                        print(f"   Send error: {msg.get('message', '')}")
                                
                                # Step 3: If WebSocket is working, test screenshot
                                if open_msgs and send_success:
                                    print(f"\n📸 STEP 3: WebSocket working, testing screenshot flow")
                                    return await test_working_screenshot_flow(websocket)
                                else:
                                    print(f"\n❌ STEP 3: WebSocket not working, cannot test screenshot")
                                    return False
                            else:
                                print(f"📋 Unexpected result format: {type(inner_result)}")
                                print(f"📋 Raw result: {str(inner_result)[:200]}...")
                                
                        except Exception as parse_error:
                            print(f"❌ Error parsing response: {parse_error}")
                            print(f"📋 Raw response: {str(result)[:300]}...")
                        
                        break
                        
                    elif result.get('type') == 'working':
                        print("⏳ Working...")
                        continue
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout {attempt + 1}/5")
                    continue
            
            return False
            
    except Exception as e:
        print(f"❌ Debug error: {e}")
        return False

async def test_working_screenshot_flow(websocket):
    """Test screenshot flow when WebSocket is confirmed working"""
    print(f"\n📸 TESTING WORKING SCREENSHOT FLOW")
    
    screenshot_js = '''
    console.log("📸 SCREENSHOT FLOW: Starting with confirmed working WebSocket");
    
    // Create test element
    const testElement = document.createElement('div');
    testElement.style.cssText = 'width:150px;height:70px;background:#00cc66;color:white;padding:12px;border-radius:6px;font-family:monospace;position:fixed;top:200px;left:200px;z-index:10000;text-align:center;font-weight:bold;';
    testElement.innerHTML = '<div>📸 WORKING FLOW</div><div>v0.2.1983</div>';
    testElement.id = 'working-flow-element';
    document.body.appendChild(testElement);
    console.log("📸 SCREENSHOT FLOW: Test element created");
    
    if (typeof html2canvas !== 'undefined') {
        html2canvas(testElement, {
            allowTaint: true,
            useCORS: true,
            scale: 1
        }).then(function(canvas) {
            console.log("✅ SCREENSHOT FLOW: Screenshot captured", canvas.width + "x" + canvas.height);
            
            const dataURL = canvas.toDataURL('image/png');
            const timestamp = Date.now();
            const filename = `working_flow_${timestamp}.png`;
            
            console.log("📸 SCREENSHOT FLOW: Filename:", filename);
            console.log("📸 SCREENSHOT FLOW: DataURL length:", dataURL.length);
            
            // Send via confirmed working WebSocket
            console.log("📤 SCREENSHOT FLOW: Sending via WebSocket...");
            const message = {
                type: 'screenshot_data',
                filename: filename,
                dataURL: dataURL,
                dimensions: {
                    width: canvas.width,
                    height: canvas.height
                },
                timestamp: timestamp,
                version: "working_flow_v0.2.1983",
                source: 'confirmed_working_websocket'
            };
            
            window.ws.send(JSON.stringify(message));
            console.log("✅ SCREENSHOT FLOW: WebSocket send completed");
            
            // Clean up
            testElement.remove();
            console.log("📸 SCREENSHOT FLOW: Cleanup completed");
            
        }).catch(function(error) {
            console.error("❌ SCREENSHOT FLOW: html2canvas failed:", error.message);
            testElement.remove();
        });
    } else {
        console.error("❌ SCREENSHOT FLOW: html2canvas not available");
        testElement.remove();
    }
    
    console.log("📸 SCREENSHOT FLOW: Flow initiated");
    return "WORKING_SCREENSHOT_FLOW_STARTED";
    '''
    
    encoded_screenshot = base64.b64encode(screenshot_js.encode()).decode()
    screenshot_command = {
        'type': 'task',
        'role': 'system',
        'task': f'[CMD:BROWSER_JS] {encoded_screenshot}'
    }
    
    print("📤 Sending working screenshot flow command...")
    await websocket.send(json.dumps(screenshot_command))
    
    # Wait for completion and check file
    await asyncio.sleep(3)
    
    # Check for working_flow files
    import os
    import glob
    
    working_files = glob.glob(".continuum/screenshots/working_flow_*.png")
    if working_files:
        latest_file = max(working_files, key=os.path.getctime)
        file_size = os.path.getsize(latest_file)
        print(f"✅ WORKING FLOW SUCCESS: {latest_file} ({file_size} bytes)")
        return True
    else:
        print("❌ WORKING FLOW: No files created")
        return False

if __name__ == "__main__":
    result = asyncio.run(debug_with_logs())
    
    print(f"\n🎯 DEBUG WITH LOGS RESULT: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 WebSocket connection and screenshot flow working!")
        print("✅ MILESTONE 6: FULLY VALIDATED")
    else:
        print("🔧 Need to fix WebSocket connection or screenshot flow")