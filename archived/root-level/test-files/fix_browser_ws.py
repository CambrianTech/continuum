#!/usr/bin/env python3
"""
Fix browser WebSocket connection using working Python debugger
"""
import asyncio
import websockets
import json
import base64

async def fix_browser_ws():
    print("🔗 FIXING BROWSER WEBSOCKET CONNECTION")
    print("=" * 50)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Python debugger connected")
            
            # Skip initial messages like working example
            for i in range(3):
                try:
                    await asyncio.wait_for(websocket.recv(), timeout=1)
                except:
                    break
            
            # Step 1: Check current WebSocket status
            print("\n🔍 STEP 1: Checking current browser WebSocket status")
            
            check_js = '''
            console.log("🔍 WS CHECK: Checking WebSocket status");
            console.log("🔍 WS CHECK: typeof window.ws:", typeof window.ws);
            console.log("🔍 WS CHECK: window.ws value:", window.ws);
            
            if (window.ws) {
                console.log("✅ WS CHECK: WebSocket exists, readyState:", window.ws.readyState);
                return "WS_EXISTS";
            } else {
                console.log("❌ WS CHECK: window.ws is null/undefined");
                return "WS_NULL";
            }
            '''
            
            encoded_js = base64.b64encode(check_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            await websocket.send(json.dumps(command))
            
            # Get check result
            ws_exists = False
            for attempt in range(5):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=2)
                    result = json.loads(response)
                    
                    if result.get('type') == 'result' and result.get('data', {}).get('role') == 'BusCommand':
                        browser_response = result.get('data', {}).get('result', {}).get('result', {}).get('browserResponse', {})
                        return_value = browser_response.get('result')
                        console_output = browser_response.get('output', [])
                        
                        for msg in console_output:
                            print(f"   {msg.get('message', '')}")
                        
                        if return_value == "WS_EXISTS":
                            print("✅ STEP 1: WebSocket already exists!")
                            ws_exists = True
                        else:
                            print("❌ STEP 1: WebSocket needs to be created")
                        break
                        
                    elif result.get('type') == 'working':
                        continue
                        
                except asyncio.TimeoutError:
                    continue
            
            if not ws_exists:
                # Step 2: Create WebSocket connection
                print("\n🔗 STEP 2: Creating browser WebSocket connection")
                
                create_js = '''
                console.log("🔗 WS CREATE: Creating WebSocket connection");
                
                try {
                    const wsProtocol = window.location.protocol === "https:" ? "wss:" : "ws:";
                    const wsUrl = wsProtocol + "//" + window.location.host;
                    console.log("🔗 WS CREATE: Connecting to:", wsUrl);
                    
                    window.ws = new WebSocket(wsUrl);
                    
                    window.ws.onopen = function(event) {
                        console.log("✅ WS CREATE: WebSocket opened successfully!");
                        console.log("✅ WS CREATE: ReadyState:", window.ws.readyState);
                    };
                    
                    window.ws.onerror = function(error) {
                        console.error("❌ WS CREATE: WebSocket error:", error);
                    };
                    
                    window.ws.onclose = function(event) {
                        console.log("🔗 WS CREATE: WebSocket closed:", event.code);
                    };
                    
                    console.log("🔗 WS CREATE: WebSocket creation initiated");
                    return "WS_CREATION_STARTED";
                    
                } catch (error) {
                    console.error("❌ WS CREATE: Failed to create WebSocket:", error);
                    return "WS_CREATION_FAILED";
                }
                '''
                
                encoded_js = base64.b64encode(create_js.encode()).decode()
                command = {
                    'type': 'task',
                    'role': 'system',
                    'task': f'[CMD:BROWSER_JS] {encoded_js}'
                }
                
                await websocket.send(json.dumps(command))
                
                # Wait for creation result
                creation_started = False
                for attempt in range(5):
                    try:
                        response = await asyncio.wait_for(websocket.recv(), timeout=2)
                        result = json.loads(response)
                        
                        if result.get('type') == 'result' and result.get('data', {}).get('role') == 'BusCommand':
                            browser_response = result.get('data', {}).get('result', {}).get('result', {}).get('browserResponse', {})
                            return_value = browser_response.get('result')
                            console_output = browser_response.get('output', [])
                            
                            for msg in console_output:
                                print(f"   {msg.get('message', '')}")
                            
                            if return_value == "WS_CREATION_STARTED":
                                print("✅ STEP 2: WebSocket creation started!")
                                creation_started = True
                            else:
                                print("❌ STEP 2: WebSocket creation failed")
                            break
                            
                        elif result.get('type') == 'working':
                            continue
                            
                    except asyncio.TimeoutError:
                        continue
                
                if creation_started:
                    # Step 3: Wait for connection and verify
                    print("\n⏳ STEP 3: Waiting for WebSocket to connect...")
                    await asyncio.sleep(3)
                    
                    verify_js = '''
                    console.log("🔍 WS VERIFY: Checking WebSocket connection status");
                    
                    if (window.ws) {
                        console.log("✅ WS VERIFY: window.ws exists!");
                        console.log("✅ WS VERIFY: ReadyState:", window.ws.readyState);
                        console.log("✅ WS VERIFY: URL:", window.ws.url);
                        
                        if (window.ws.readyState === WebSocket.OPEN) {
                            console.log("🎉 WS VERIFY: WebSocket is CONNECTED!");
                            return "WS_CONNECTED";
                        } else {
                            console.log("⏳ WS VERIFY: WebSocket connecting...", window.ws.readyState);
                            return "WS_CONNECTING";
                        }
                    } else {
                        console.log("❌ WS VERIFY: window.ws still undefined");
                        return "WS_FAILED";
                    }
                    '''
                    
                    encoded_js = base64.b64encode(verify_js.encode()).decode()
                    command = {
                        'type': 'task',
                        'role': 'system',
                        'task': f'[CMD:BROWSER_JS] {encoded_js}'
                    }
                    
                    await websocket.send(json.dumps(command))
                    
                    # Get verification result
                    for attempt in range(5):
                        try:
                            response = await asyncio.wait_for(websocket.recv(), timeout=2)
                            result = json.loads(response)
                            
                            if result.get('type') == 'result' and result.get('data', {}).get('role') == 'BusCommand':
                                browser_response = result.get('data', {}).get('result', {}).get('result', {}).get('browserResponse', {})
                                return_value = browser_response.get('result')
                                console_output = browser_response.get('output', [])
                                
                                for msg in console_output:
                                    print(f"   {msg.get('message', '')}")
                                
                                if return_value == "WS_CONNECTED":
                                    print("🎉 STEP 3: Browser WebSocket connection SUCCESS!")
                                    print("✅ window.ws is now connected and ready")
                                    print("✅ Screenshots can now be sent via WebSocket")
                                    print("✅ MILESTONE 6 unblocked!")
                                    return True
                                elif return_value == "WS_CONNECTING":
                                    print("⏳ STEP 3: WebSocket still connecting, may need more time")
                                    return False
                                else:
                                    print("❌ STEP 3: WebSocket connection failed")
                                    return False
                                
                            elif result.get('type') == 'working':
                                continue
                                
                        except asyncio.TimeoutError:
                            continue
            else:
                print("✅ Browser WebSocket already exists and ready!")
                return True
            
            return False
            
    except Exception as e:
        print(f"❌ Error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(fix_browser_ws())
    
    print(f"\n🎯 BROWSER WEBSOCKET FIX: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 Browser WebSocket connection established!")
        print("✅ window.ws is now available in browser")
        print("✅ Ready to complete MILESTONE 6 screenshot functionality")
    else:
        print("🔧 Browser WebSocket connection needs more debugging")