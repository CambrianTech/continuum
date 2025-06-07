#!/usr/bin/env python3
"""
Debug the Promise Post Office System
"""

import asyncio
import sys
from continuum_client import ContinuumClient, ContinuumServerManager

async def debug_promise_system():
    """Debug the complete Promise Post Office flow"""
    
    print("🔧 Starting Continuum server for debugging...")
    
    with ContinuumServerManager(port=5559) as server:
        print("✅ Server started successfully")
        
        print("🔌 Connecting to Continuum...")
        async with ContinuumClient("ws://localhost:5559") as client:
            print("✅ Connected to Continuum")
            
            # Register test agent
            print("🤖 Registering debug agent...")
            await client.register_agent({
                'agentId': 'debug-agent',
                'agentName': 'Debug Agent',
                'agentType': 'ai',
                'capabilities': ['debugging', 'promise-post-office']
            })
            print("✅ Agent registered")
            
            # Test 1: Simple JavaScript execution
            print("\n📤 Test 1: Simple JavaScript execution")
            try:
                result = await client.js.get_value("return 'Hello Promise System!'", timeout=15)
                print(f"📥 Success: {result}")
            except Exception as e:
                print(f"❌ Failed: {e}")
            
            # Test 2: Math calculation
            print("\n📤 Test 2: Math calculation")
            try:
                result = await client.js.get_value("return 2 + 3", timeout=15)
                print(f"📥 Success: {result}")
            except Exception as e:
                print(f"❌ Failed: {e}")
            
            # Test 3: Error handling
            print("\n📤 Test 3: Error handling")
            try:
                result = await client.js.get_value("return undefined_var.property", timeout=15)
                print(f"📥 Unexpected success: {result}")
            except Exception as e:
                print(f"📥 Expected error: {e}")
            
            print("\n🎉 Debug complete!")

if __name__ == "__main__":
    asyncio.run(debug_promise_system())