#!/usr/bin/env python3
"""
Test Screenshot Command for JTAG Visual Validation
"""

import asyncio
import websockets
import json

async def test_screenshot():
    print("📸 Testing Screenshot Command for JTAG Visual Validation")
    print("=" * 60)
    
    try:
        ws = await websockets.connect("ws://localhost:9000")
        print("✅ Connected to WebSocket daemon")
        
        # Initialize
        await ws.send(json.dumps({
            "type": "client_init",
            "data": {
                "userAgent": "Screenshot Test",
                "url": "test://screenshot",
                "timestamp": "2025-06-29T21:56:00.000Z"
            }
        }))
        
        response = await ws.recv()
        init_data = json.loads(response)
        client_id = init_data["data"]["clientId"]
        print(f"✅ Client initialized: {client_id}")
        
        # Test screenshot command
        print("\n📸 Executing screenshot command...")
        await ws.send(json.dumps({
            "type": "execute_command",
            "data": {
                "command": "screenshot",
                "args": {
                    "target": "browser",
                    "format": "png",
                    "quality": 90
                }
            },
            "timestamp": "2025-06-29T21:56:00.000Z",
            "clientId": client_id
        }))
        
        response = await asyncio.wait_for(ws.recv(), timeout=10.0)
        result = json.loads(response)
        
        print(f"📥 Response type: {result.get('type')}")
        print(f"📊 Processed by: {result.get('processedBy')}")
        
        if result.get('data', {}).get('success'):
            print("✅ Screenshot command executed successfully!")
            data = result.get('data', {})
            if 'screenshotPath' in data:
                print(f"📁 Screenshot saved to: {data['screenshotPath']}")
            if 'base64' in data:
                print(f"🖼️ Base64 screenshot data length: {len(data['base64'])} chars")
            print(f"📏 Dimensions: {data.get('width', 'unknown')}x{data.get('height', 'unknown')}")
        else:
            print(f"❌ Screenshot failed: {result.get('data', {}).get('error', 'unknown error')}")
            
        await ws.close()
        return result.get('data', {}).get('success', False)
        
    except Exception as e:
        print(f"❌ Screenshot test failed: {e}")
        return False

if __name__ == "__main__":
    success = asyncio.run(test_screenshot())
    if success:
        print("\n🎯 SCREENSHOT CAPABILITY VERIFIED - JTAG Visual Validation Ready!")
    else:
        print("\n🔧 Screenshot capability needs work")