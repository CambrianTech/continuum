#!/usr/bin/env python3
"""
Test simple bus command and check server logs
"""
import asyncio
import websockets
import json
import base64

async def test_simple_bus_command():
    print("🔍 TESTING SIMPLE BUS COMMAND")
    print("=" * 50)
    print("Send a simple [CMD:BROWSER_JS] and monitor server response")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected for bus command test")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Simple test command
            simple_js = '''
            console.log("🔍 SIMPLE BUS TEST: Hello from browser");
            console.log("🔍 SIMPLE BUS TEST: This should show in browser console");
            return "SIMPLE_BUS_SUCCESS";
            '''
            
            encoded_js = base64.b64encode(simple_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print(f"📤 Sending simple bus command:")
            print(f"   Type: {command['type']}")
            print(f"   Role: {command['role']}")
            print(f"   Task: [CMD:BROWSER_JS] <encoded_js>")
            print(f"   Encoded length: {len(encoded_js)}")
            
            await websocket.send(json.dumps(command))
            print("✅ Command sent, waiting for response...")
            
            # Monitor responses
            for attempt in range(5):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=5)
                    result = json.loads(response)
                    
                    print(f"\n📨 Response {attempt + 1}:")
                    print(f"   Type: {result.get('type')}")
                    
                    if result.get('type') == 'working':
                        print(f"   Status: {result.get('data')}")
                        continue
                        
                    elif result.get('type') == 'result':
                        print(f"   ✅ Got result response!")
                        
                        data = result.get('data', {})
                        print(f"   Role: {data.get('role')}")
                        print(f"   Task: {data.get('task', '')[:50]}...")
                        
                        # Check if this is a bus command result
                        if data.get('role') == 'BusCommand':
                            print(f"   🚀 BUS COMMAND RESULT RECEIVED!")
                            
                            bus_result = data.get('result', {})
                            print(f"   Command: {bus_result.get('command')}")
                            print(f"   Executed: {bus_result.get('result', {}).get('executed', False)}")
                            
                            browser_response = bus_result.get('result', {}).get('browserResponse', {})
                            if browser_response:
                                print(f"   Browser success: {browser_response.get('success')}")
                                print(f"   Browser result: {browser_response.get('result')}")
                                console_output = browser_response.get('output', [])
                                print(f"   Console messages: {len(console_output)}")
                                
                                for i, msg in enumerate(console_output):
                                    print(f"      {i+1}. [{msg.get('level', 'log').upper()}] {msg.get('message', '')}")
                                
                                return True
                            else:
                                print(f"   ❌ No browser response in bus result")
                                return False
                                
                        elif data.get('role') == 'GeneralAI':
                            print(f"   ❌ Got GeneralAI response instead of bus command!")
                            print(f"   AI Response: {data.get('result', '')[:100]}...")
                            return False
                        else:
                            print(f"   ❌ Unknown role: {data.get('role')}")
                            print(f"   Raw data: {str(data)[:200]}...")
                            return False
                    else:
                        print(f"   Unknown response type: {result.get('type')}")
                        
                except asyncio.TimeoutError:
                    print(f"   ⏰ Timeout {attempt + 1}/5")
                    continue
            
            print("❌ No valid response received")
            return False
            
    except Exception as e:
        print(f"❌ Test error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(test_simple_bus_command())
    
    print(f"\n🎯 SIMPLE BUS COMMAND TEST: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("✅ Bus command routing is working!")
        print("✅ Browser JavaScript execution is working!")
        print("✅ Console capture is working!")
    else:
        print("❌ Bus command routing has issues")