#!/usr/bin/env python3
"""
Read the debug logs I wrote to myself
"""
import asyncio
import websockets
import json
import base64

async def read_debug_logs():
    uri = "ws://localhost:9000"
    
    async with websockets.connect(uri) as websocket:
        await websocket.recv()
        await websocket.recv()
        
        # Simple script to capture current console state
        read_js = """
        console.log("READING_DEBUG_LOGS: Checking for previous debug messages");
        return "CONSOLE_READ_ATTEMPT";
        """
        
        encoded = base64.b64encode(read_js.encode()).decode()
        command = {
            'type': 'task',
            'role': 'system',
            'task': f'[CMD:BROWSER_JS] {encoded}'
        }
        
        print("📖 Reading debug logs from console...")
        await websocket.send(json.dumps(command))
        
        # Get the result with console output
        for attempt in range(3):
            response = await websocket.recv()
            data = json.loads(response)
            
            if data.get('type') == 'result':
                try:
                    # Parse the nested structure to get console output
                    result_data = data.get('data', {})
                    inner_result = result_data.get('result', {})
                    browser_result = inner_result.get('result', {})
                    browser_response = browser_result.get('browserResponse', {})
                    console_output = browser_response.get('output', [])
                    
                    print(f"\n📋 DEBUG LOGS FROM BROWSER CONSOLE:")
                    print("=" * 60)
                    
                    debug_messages = []
                    for msg in console_output:
                        message = msg.get('message', '')
                        if 'CLAUDE_DEBUG' in message:
                            debug_messages.append(message)
                            print(f"🔍 {message}")
                    
                    if not debug_messages:
                        print("📝 No CLAUDE_DEBUG messages found in recent console output")
                        print(f"📝 Total console messages: {len(console_output)}")
                        print("📝 Recent messages:")
                        for msg in console_output[-5:]:  # Show last 5
                            print(f"   - {msg.get('message', '')}")
                    else:
                        print(f"\n📊 Found {len(debug_messages)} debug messages")
                    
                    print("=" * 60)
                    break
                    
                except Exception as e:
                    print(f"❌ Error parsing console logs: {e}")
                    break
                    
            elif data.get('type') == 'working':
                continue
        
        return True

if __name__ == "__main__":
    asyncio.run(read_debug_logs())