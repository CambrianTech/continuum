#!/usr/bin/env python3
"""Test Continuum connection and take a screenshot"""

import asyncio
import json
import websockets

async def test_continuum():
    # Connect to Continuum
    ws = await websockets.connect("ws://localhost:9000")
    print("✅ Connected to Continuum")
    
    # Join as Claude
    greeting = {
        "type": "join", 
        "agent": "Claude",
        "timestamp": asyncio.get_event_loop().time()
    }
    await ws.send(json.dumps(greeting))
    print("✅ Joined as Claude")
    
    # Take a screenshot
    screenshot_cmd = {
        "type": "command",
        "command": "SCREENSHOT", 
        "agent": "Claude",
        "filename": "claude_test.png",
        "timestamp": asyncio.get_event_loop().time()
    }
    await ws.send(json.dumps(screenshot_cmd))
    print("📸 Screenshot command sent")
    
    # Listen for response
    try:
        response = await asyncio.wait_for(ws.recv(), timeout=5.0)
        data = json.loads(response)
        print(f"📥 Response: {data.get('type')}")
        if data.get('type') == 'command_result':
            print(f"✅ Screenshot result: {data.get('result')}")
    except asyncio.TimeoutError:
        print("⏱️ No response received")
    
    await ws.close()
    print("👋 Disconnected")

if __name__ == "__main__":
    asyncio.run(test_continuum())