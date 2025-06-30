#!/usr/bin/env python3
"""
Working WebSocket Client - Uses discovered message types
"""

import asyncio
import json
import websockets
from datetime import datetime
import uuid

class WorkingContinuumClient:
    """WebSocket client using the correct protocol"""
    
    def __init__(self, ws_url="ws://localhost:9000"):
        self.ws_url = ws_url
        self.client_id = None
        self.available_types = []
        
    async def connect(self):
        """Connect and get client ID"""
        self.ws = await websockets.connect(self.ws_url)
        
        # Send client_init
        await self.ws.send(json.dumps({
            "type": "client_init", 
            "data": {
                "userAgent": "Python Continuum Client",
                "url": "python://working_client",
                "timestamp": datetime.now().isoformat()
            }
        }))
        
        # Get connection confirmation
        response = await self.ws.recv()
        message = json.loads(response)
        
        if message.get("type") == "connection_confirmed":
            self.client_id = message.get("data", {}).get("clientId")
            print(f"✅ Connected with client ID: {self.client_id}")
            return self
        else:
            raise ConnectionError("Failed to get connection confirmation")
    
    async def send_message(self, message_type: str, data: dict = None):
        """Send message with proper format"""
        message = {
            "type": message_type,
            "timestamp": datetime.now().isoformat(),
            "clientId": self.client_id
        }
        
        if data:
            message["data"] = data
            
        await self.ws.send(json.dumps(message))
        
        # Get response
        response = await self.ws.recv()
        return json.loads(response)
    
    async def get_stats(self):
        """Get system stats"""
        return await self.send_message("get_stats")
    
    async def get_capabilities(self):
        """Get system capabilities"""
        return await self.send_message("get_capabilities")
    
    async def execute_command(self, command: str, args: dict = None):
        """Execute a command"""
        return await self.send_message("execute_command", {
            "command": command,
            "args": args or {}
        })
    
    async def ping(self):
        """Send ping"""
        return await self.send_message("ping")
    
    async def close(self):
        """Close connection"""
        if self.ws:
            await self.ws.close()

async def demo():
    """Demonstrate working WebSocket client"""
    print("🌟 Working Continuum WebSocket Client")
    print("=" * 50)
    
    client = WorkingContinuumClient()
    await client.connect()
    
    # Test ping
    print("\n🏓 Testing ping...")
    ping_result = await client.ping()
    print(f"Ping result: {ping_result}")
    
    # Test stats
    print("\n📊 Getting stats...")
    stats = await client.get_stats()
    print(f"Stats: {stats}")
    
    # Test capabilities
    print("\n🔍 Getting capabilities...")
    capabilities = await client.get_capabilities()
    print(f"Capabilities: {capabilities}")
    
    # Test command execution
    print("\n⚡ Testing command execution...")
    try:
        health_result = await client.execute_command("health")
        print(f"Health command: {health_result}")
    except Exception as e:
        print(f"Health command failed: {e}")
    
    await client.close()
    print("\n✅ Demo complete!")

if __name__ == "__main__":
    asyncio.run(demo())