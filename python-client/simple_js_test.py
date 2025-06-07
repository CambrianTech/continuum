#!/usr/bin/env python3
"""
Simple test to verify browser JavaScript execution
"""

import asyncio
import json
import websockets
from continuum_client.utils import get_continuum_ws_url, load_continuum_config

async def simple_test():
    load_continuum_config()
    ws_url = get_continuum_ws_url()
    
    print(f"Testing {ws_url}")
    print("Browser should be open at http://localhost:5555/")
    
    async with websockets.connect(ws_url) as ws:
        # Skip initial messages
        await ws.recv()
        await ws.recv()
        
        # Send simple task
        task = {
            'type': 'task',
            'role': 'system',
            'task': '[CMD:BROWSER_JS] cmV0dXJuICdoZWxsbycK'  # return 'hello'
        }
        
        print(f"Sending: {task}")
        await ws.send(json.dumps(task))
        
        # Listen for response
        print("Waiting for response...")
        try:
            response = await asyncio.wait_for(ws.recv(), timeout=5.0)
            print(f"Got response: {response}")
        except asyncio.TimeoutError:
            print("No response received")

if __name__ == "__main__":
    asyncio.run(simple_test())