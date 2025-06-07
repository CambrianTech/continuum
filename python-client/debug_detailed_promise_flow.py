#!/usr/bin/env python3
"""
Debug the complete Promise Post Office System with detailed logging
"""

import asyncio
import json
import websockets
import time
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config, get_continuum_ws_url

async def debug_complete_flow():
    """Debug with maximum detail and logging"""
    
    # Load config
    load_continuum_config()
    ws_url = get_continuum_ws_url()
    
    print(f"🔧 Debugging Promise Post Office System")
    print(f"📍 WebSocket URL: {ws_url}")
    print(f"🌐 HTML Interface: http://localhost:5555/")
    print()
    
    print("🔌 Connecting to Continuum...")
    try:
        async with ContinuumClient() as client:
            print(f"✅ Connected to {client.url}")
            print(f"🔗 WebSocket connection: {client.ws}")
            print()
            
            # Register test agent with detailed info
            agent_id = f"debug-agent-{int(time.time())}"
            print(f"🤖 Registering agent: {agent_id}")
            
            await client.register_agent({
                'agentId': agent_id,
                'agentName': 'Debug Promise Agent',
                'agentType': 'ai',
                'capabilities': ['debugging', 'promise-post-office', 'js-execution']
            })
            print("✅ Agent registered successfully")
            print()
            
            # Test 1: Simple JavaScript with detailed timing
            print("📤 Test 1: Simple JavaScript execution")
            print("   Command: return 'Hello Promise Debug!'")
            
            start_time = time.time()
            try:
                result = await client.js.get_value("return 'Hello Promise Debug!'", timeout=15)
                end_time = time.time()
                print(f"📥 SUCCESS: {result}")
                print(f"⏱️  Execution time: {end_time - start_time:.2f}s")
            except Exception as e:
                end_time = time.time()
                print(f"❌ FAILED: {e}")
                print(f"⏱️  Time until failure: {end_time - start_time:.2f}s")
            print()
            
            # Test 2: Check if browser is connected
            print("📤 Test 2: Browser connectivity check")
            print("   Command: return typeof window !== 'undefined' ? 'browser' : 'no-browser'")
            
            start_time = time.time()
            try:
                result = await client.js.get_value("return typeof window !== 'undefined' ? 'browser' : 'no-browser'", timeout=15)
                end_time = time.time()
                print(f"📥 SUCCESS: {result}")
                print(f"⏱️  Execution time: {end_time - start_time:.2f}s")
            except Exception as e:
                end_time = time.time()
                print(f"❌ FAILED: {e}")
                print(f"⏱️  Time until failure: {end_time - start_time:.2f}s")
            print()
            
            # Test 3: Low-level WebSocket message inspection
            print("📤 Test 3: Direct WebSocket message send")
            print("   Sending raw task message...")
            
            raw_task = {
                'type': 'task',
                'role': 'system',
                'task': '[CMD:BROWSER_JS] cmV0dXJuICdyYXcgdGVzdCc='  # base64 for "return 'raw test'"
            }
            
            print(f"   Raw message: {json.dumps(raw_task, indent=2)}")
            
            await client.ws.send(json.dumps(raw_task))
            print("✅ Raw message sent")
            
            # Wait for any response
            print("👂 Listening for WebSocket responses...")
            try:
                # Set a shorter timeout for this test
                response = await asyncio.wait_for(client.ws.recv(), timeout=5.0)
                print(f"📥 WebSocket response: {response}")
            except asyncio.TimeoutError:
                print("⏰ No WebSocket response received within 5s")
            print()
            
    except Exception as e:
        print(f"💥 Connection failed: {e}")
        return
    
    print("🔚 Debug complete!")
    print()
    print("🔍 Next steps if issues found:")
    print("   1. Check if browser is open at http://localhost:5555/")
    print("   2. Check browser console for JavaScript errors")
    print("   3. Check Continuum server logs")
    print("   4. Verify WebSocket message routing")

if __name__ == "__main__":
    asyncio.run(debug_complete_flow())