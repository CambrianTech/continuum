#!/usr/bin/env python3
"""
Simple screenshot debug - just send command and see what happens
"""

import asyncio
import websockets
import json
import base64

async def simple_screenshot_debug():
    print("🔍 SIMPLE SCREENSHOT DEBUG")
    print("=" * 30)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Send simple screenshot test
            screenshot_test = """
            console.log("🧪 SIMPLE SCREENSHOT DEBUG");
            console.log("html2canvas available:", typeof html2canvas !== 'undefined');
            console.log("WebSocket available:", typeof window.ws !== 'undefined');
            
            if (typeof html2canvas === 'undefined') {
                console.error("❌ html2canvas NOT LOADED!");
                return { error: "html2canvas_missing" };
            }
            
            console.log("📸 Attempting basic screenshot...");
            
            // Try the simplest possible screenshot
            html2canvas(document.body, {
                scale: 0.1,
                allowTaint: true
            }).then(canvas => {
                console.log("✅ Screenshot SUCCESS:", canvas.width, "x", canvas.height);
                return { success: true, width: canvas.width, height: canvas.height };
            }).catch(error => {
                console.error("❌ Screenshot FAILED:", error.message);
                return { success: false, error: error.message };
            });
            
            return "SCREENSHOT_TEST_SENT";
            """
            
            encoded = base64.b64encode(screenshot_test.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded}'
            }
            
            print("📤 Sending screenshot test...")
            await websocket.send(json.dumps(command))
            
            # Wait for result
            for i in range(5):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=3)
                    data = json.loads(response)
                    
                    if data.get('type') == 'result':
                        print("📥 Got result!")
                        # Navigate the nested structure safely
                        result_data = data.get('data', {})
                        if isinstance(result_data, dict):
                            inner_result = result_data.get('result', {})
                            if isinstance(inner_result, dict):
                                browser_response = inner_result.get('result', {})
                                if isinstance(browser_response, dict):
                                    browser_resp = browser_response.get('browserResponse', {})
                                    if isinstance(browser_resp, dict):
                                        console_output = browser_resp.get('output', [])
                                        return_value = browser_resp.get('result')
                                        
                                        print(f"Return value: {return_value}")
                                        print(f"Console messages: {len(console_output)}")
                                        
                                        for msg in console_output:
                                            if isinstance(msg, dict):
                                                level = msg.get('level', 'unknown')
                                                message = msg.get('message', '')
                                                print(f"  [{level}] {message}")
                        break
                    else:
                        print(f"📥 {data.get('type')}: {str(data)[:100]}")
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout {i+1}")
                    
            return True
            
    except Exception as e:
        print(f"❌ Error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(simple_screenshot_debug())
    print(f"\n🎯 Result: {'SUCCESS' if result else 'FAILED'}")