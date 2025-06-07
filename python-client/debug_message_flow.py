#!/usr/bin/env python3
"""
Debug WebSocket message flow without conflicting recv calls
"""

import asyncio
import json
import websockets
import time
from continuum_client.utils import get_continuum_ws_url, load_continuum_config

async def debug_raw_websocket():
    """Test raw WebSocket connection without ContinuumClient wrapper"""
    
    load_continuum_config()
    ws_url = get_continuum_ws_url()
    
    print(f"🔧 Testing raw WebSocket connection to {ws_url}")
    
    try:
        async with websockets.connect(ws_url) as ws:
            print("✅ Raw WebSocket connected")
            
            # Skip initial messages
            await ws.recv()  # status
            await ws.recv()  # banner
            print("📝 Skipped initial messages")
            
            # Register agent
            agent_msg = {
                'type': 'agent_registration',
                'agentId': f'raw-debug-{int(time.time())}',
                'agentName': 'Raw Debug Agent',
                'agentType': 'ai'
            }
            
            print(f"📤 Sending agent registration...")
            await ws.send(json.dumps(agent_msg))
            
            # Send JavaScript task
            js_task = {
                'type': 'task',
                'role': 'system', 
                'task': '[CMD:BROWSER_JS] cmV0dXJuICdyYXcgdGVzdCc='  # "return 'raw test'"
            }
            
            print(f"📤 Sending JavaScript task...")
            await ws.send(json.dumps(js_task))
            
            # Listen for responses
            print("👂 Listening for responses...")
            
            try:
                for i in range(5):  # Listen for up to 5 messages
                    response = await asyncio.wait_for(ws.recv(), timeout=3.0)
                    print(f"📥 Response {i+1}: {response}")
                    
                    # Parse and check for JavaScript result
                    try:
                        data = json.loads(response)
                        if 'result' in data or 'error' in data:
                            print(f"🎯 Found result: {data}")
                            break
                    except:
                        pass
                        
            except asyncio.TimeoutError:
                print("⏰ No more responses within timeout")
                
    except Exception as e:
        print(f"❌ WebSocket error: {e}")

if __name__ == "__main__":
    asyncio.run(debug_raw_websocket())