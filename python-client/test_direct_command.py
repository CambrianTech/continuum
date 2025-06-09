#!/usr/bin/env python3
"""
Test direct command execution
"""

import asyncio
import websockets
import json

async def test_direct_command():
    print("🔧 DIRECT COMMAND TEST")
    print("=" * 40)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected")
            
            # Collect all initial messages
            initial_messages = []
            for i in range(3):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=1.0)
                    result = json.loads(response)
                    print(f"📥 Initial message {i+1}: {result.get('type')} - {result}")
                    initial_messages.append(result)
                except asyncio.TimeoutError:
                    print(f"⏰ No more initial messages")
                    break
            
            # Test simple known command that should work
            command = {
                'type': 'task',
                'role': 'system',
                'task': '[CMD:EXEC] echo "Bus command test"'
            }
            
            print(f"📤 Sending: {command}")
            await websocket.send(json.dumps(command))
            
            # Wait for responses - increase timeout for command execution
            print("⏳ Waiting for command execution...")
            for i in range(10):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=30.0)
                    result = json.loads(response)
                    print(f"📥 Response {i+1}: {result.get('type')} - {result}")
                    
                    if result.get('type') == 'result':
                        print(f"🎯 Final result: {result}")
                        if result.get('data', {}).get('role') == 'BusCommand':
                            print("✅ SUCCESS: Bus command executed!")
                            break
                        elif 'BusCommand' in str(result):
                            print("✅ SUCCESS: Bus command result detected!")
                            break
                        else:
                            print("⚠️  Non-bus command result, continuing...")
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout on response {i+1}")
                    break
                except Exception as e:
                    print(f"❌ Error processing response {i+1}: {e}")
                    break
                    
    except Exception as e:
        print(f"❌ Error: {e}")

if __name__ == "__main__":
    asyncio.run(test_direct_command())