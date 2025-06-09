#!/usr/bin/env python3
"""
Simple Continuum Python Client
=============================
Lightweight client for Continuum debugging and widget development.
"""

import asyncio
import json
import websockets
import sys
from pathlib import Path

class SimpleContinuumClient:
    def __init__(self, agent_name="Claude"):
        self.agent_name = agent_name
        self.ws = None
        self.connected = False
        
    async def connect(self, url="ws://localhost:9000"):
        """Connect to Continuum WebSocket server"""
        try:
            print(f"Connecting to {url}...")
            self.ws = await websockets.connect(url)
            self.connected = True
            print("✅ Connected to Continuum")
            
            # Send greeting
            greeting = {
                "type": "join",
                "agent": self.agent_name,
                "timestamp": asyncio.get_event_loop().time()
            }
            await self.ws.send(json.dumps(greeting))
            print(f"✅ Joined as {self.agent_name}")
            
            return True
        except Exception as e:
            print(f"❌ Connection failed: {e}")
            return False
    
    async def send_command(self, command, **kwargs):
        """Send a command to Continuum bus"""
        if not self.connected:
            print("❌ Not connected")
            return None
            
        message = {
            "type": "command",
            "command": command,
            "agent": self.agent_name,
            "timestamp": asyncio.get_event_loop().time(),
            **kwargs
        }
        
        await self.ws.send(json.dumps(message))
        print(f"📤 Sent: {command}")
    
    async def take_screenshot(self, filename="debug_screenshot.png"):
        """Take a screenshot via Continuum"""
        await self.send_command("SCREENSHOT", filename=filename)
        print(f"📸 Screenshot requested: {filename}")
    
    async def listen(self):
        """Listen for messages from Continuum"""
        try:
            async for message in self.ws:
                data = json.loads(message)
                print(f"📥 Received: {data.get('type', 'unknown')}")
                
                # Handle specific message types
                if data.get('type') == 'command_result':
                    print(f"✅ Command result: {data.get('result', 'success')}")
                elif data.get('type') == 'error':
                    print(f"❌ Error: {data.get('message', 'unknown error')}")
                    
        except websockets.exceptions.ConnectionClosed:
            print("🔌 Connection closed")
            self.connected = False
        except Exception as e:
            print(f"❌ Listen error: {e}")
    
    async def chat(self, message):
        """Send a chat message"""
        await self.send_command("CHAT", message=message)
    
    async def run_interactive(self):
        """Run interactive session"""
        print("\n🚀 Continuum Python Client Ready!")
        print("Commands: screenshot, chat <message>, quit")
        
        # Start listener
        listen_task = asyncio.create_task(self.listen())
        
        while self.connected:
            try:
                cmd = input("\n> ").strip()
                
                if cmd == "quit":
                    break
                elif cmd == "screenshot":
                    await self.take_screenshot()
                elif cmd.startswith("chat "):
                    message = cmd[5:]
                    await self.chat(message)
                elif cmd == "help":
                    print("Available commands:")
                    print("  screenshot - Take screenshot")
                    print("  chat <msg> - Send chat message")
                    print("  quit - Exit")
                else:
                    print(f"Unknown command: {cmd}")
                    
            except KeyboardInterrupt:
                break
                
        listen_task.cancel()
        await self.ws.close()
        print("👋 Disconnected")

async def main():
    client = SimpleContinuumClient("Claude")
    
    # Try to connect
    if await client.connect():
        await client.run_interactive()
    else:
        print("Failed to connect to Continuum")
        sys.exit(1)

if __name__ == "__main__":
    asyncio.run(main())