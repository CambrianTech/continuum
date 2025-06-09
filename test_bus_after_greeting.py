#!/usr/bin/env python3
"""
Test bus command after greeting settles
"""
import asyncio
import websockets
import json
import base64

async def test_bus_after_greeting():
    print("🔍 TESTING BUS COMMAND AFTER GREETING")
    print("=" * 50)
    print("Wait for greeting to settle, then send bus command")
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected, waiting for all initial messages...")
            
            # Wait for and consume ALL initial messages (banner, status, greeting)
            initial_messages = []
            for i in range(5):
                try:
                    message = await asyncio.wait_for(websocket.recv(), timeout=2)
                    result = json.loads(message)
                    initial_messages.append(result)
                    print(f"📥 Initial message {i+1}: {result.get('type')} - {result.get('data', {}).get('role', 'no role')}")
                except asyncio.TimeoutError:
                    print(f"⏰ No more initial messages after {i+1}")
                    break
            
            print(f"\n✅ Consumed {len(initial_messages)} initial messages")
            print("🎯 Now sending bus command on clean connection...")
            
            # Now send the bus command
            test_js = '''
            console.log("🚀 BUS TEST AFTER GREETING: Hello from clean connection");
            console.log("🚀 BUS TEST: Browser console working");
            return "BUS_AFTER_GREETING_SUCCESS";
            '''
            
            encoded_js = base64.b64encode(test_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            print(f"📤 Sending bus command:")
            print(f"   Task: [CMD:BROWSER_JS] <encoded>")
            
            await websocket.send(json.dumps(command))
            print("✅ Command sent, monitoring responses...")
            
            # Monitor responses carefully
            for attempt in range(8):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=3)
                    result = json.loads(response)
                    
                    print(f"\n📨 Response {attempt + 1}:")
                    print(f"   Type: {result.get('type')}")
                    
                    if result.get('type') == 'working':
                        data = result.get('data', '')
                        print(f"   Working: {data}")
                        
                        # Check if it mentions our command
                        if '[CMD:BROWSER_JS]' in data:
                            print(f"   ✅ Working on our bus command!")
                        else:
                            print(f"   ⚠️  Working on something else...")
                        continue
                        
                    elif result.get('type') == 'result':
                        data = result.get('data', {})
                        role = data.get('role')
                        task = data.get('task', '')
                        
                        print(f"   Role: {role}")
                        print(f"   Task: {task[:50]}...")
                        
                        # Check if this is our bus command result
                        if role == 'BusCommand':
                            print(f"   🎉 SUCCESS: Got BusCommand result!")
                            
                            bus_result = data.get('result', {})
                            print(f"   Command: {bus_result.get('command')}")
                            
                            if 'result' in bus_result and 'browserResponse' in bus_result['result']:
                                browser_response = bus_result['result']['browserResponse']
                                print(f"   Browser success: {browser_response.get('success')}")
                                print(f"   Browser result: {browser_response.get('result')}")
                                
                                console_output = browser_response.get('output', [])
                                print(f"   Console messages: {len(console_output)}")
                                
                                for msg in console_output:
                                    if 'BUS TEST' in msg.get('message', ''):
                                        print(f"      ✅ {msg.get('message')}")
                                
                                return True
                            else:
                                print(f"   ❌ BusCommand result missing browser response")
                                print(f"   Raw result: {str(bus_result)[:200]}...")
                                return False
                                
                        elif task == 'user_connection_greeting':
                            print(f"   ⚠️  Got another greeting (ignoring)")
                            continue
                            
                        else:
                            print(f"   ❌ Unexpected result - Role: {role}, Task: {task}")
                            print(f"   Result: {str(data.get('result', ''))[:100]}...")
                            return False
                    
                    else:
                        print(f"   Unknown type: {result.get('type')}")
                        
                except asyncio.TimeoutError:
                    print(f"   ⏰ Timeout {attempt + 1}/8")
                    continue
            
            print("❌ No valid bus command response received")
            return False
            
    except Exception as e:
        print(f"❌ Test error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(test_bus_after_greeting())
    
    print(f"\n🎯 BUS AFTER GREETING TEST: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 Bus command routing FIXED!")
        print("✅ Ready to complete all milestone validations!")
    else:
        print("🔧 Bus routing still needs investigation")