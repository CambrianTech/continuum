#!/usr/bin/env python3
"""
Simple test of BROWSER_JS command
"""

import asyncio
import websockets
import json
import base64

async def test_browser_js():
    print("🔧 SIMPLE BROWSER_JS TEST")
    print("=" * 40)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Test simple BROWSER_JS command
            js_code = "console.log('TEST'); return 'SUCCESS';"
            encoded = base64.b64encode(js_code.encode()).decode()
            
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded}'
            }
            
            print(f"📤 Sending: {command}")
            await websocket.send(json.dumps(command))
            
            # Wait for responses
            for i in range(3):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=5.0)
                    result = json.loads(response)
                    print(f"📥 Response {i+1}: {result.get('type')} - {result}")
                    
                    if result.get('type') == 'result':
                        break
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout on response {i+1}")
                    break
                    
    except Exception as e:
        print(f"❌ Error: {e}")

if __name__ == "__main__":
    asyncio.run(test_browser_js())