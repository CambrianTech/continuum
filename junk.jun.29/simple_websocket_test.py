#!/usr/bin/env python3
"""
Simple WebSocket test using the exact protocol from browser client
"""

import asyncio
import json
import websockets
from datetime import datetime
import uuid

async def test_websocket():
    """Test exact WebSocket protocol from browser client"""
    print("🌐 Testing WebSocket connection to Continuum...")
    
    try:
        # Connect to WebSocket
        async with websockets.connect("ws://localhost:9000") as websocket:
            print("✅ WebSocket connected")
            
            # Send client_init message like browser
            client_init = {
                "type": "client_init",
                "data": {
                    "userAgent": "Python WebSocket Test",
                    "url": "python://simple_test",
                    "timestamp": datetime.now().isoformat()
                }
            }
            
            await websocket.send(json.dumps(client_init))
            print("📤 Sent client_init")
            
            # Wait for connection_confirmed
            response = await websocket.recv()
            message = json.loads(response)
            print(f"📥 Received: {message}")
            
            if message.get("type") == "connection_confirmed":
                client_id = message.get("data", {}).get("clientId")
                print(f"🆔 Client ID: {client_id}")
                
                # Try executing health command like browser
                request_id = f"cmd_{int(datetime.now().timestamp() * 1000)}_{uuid.uuid4().hex[:9]}"
                
                health_command = {
                    "type": "execute_command", 
                    "data": {
                        "command": "health",
                        "params": "{}",
                        "requestId": request_id
                    },
                    "timestamp": datetime.now().isoformat(),
                    "clientId": client_id
                }
                
                await websocket.send(json.dumps(health_command))
                print("📤 Sent health command")
                
                # Wait for response
                response = await websocket.recv()
                result = json.loads(response)
                print(f"📥 Health result: {result}")
                
            else:
                print(f"❌ Unexpected message type: {message.get('type')}")
                
    except Exception as e:
        print(f"❌ WebSocket test failed: {e}")

if __name__ == "__main__":
    asyncio.run(test_websocket())