#!/usr/bin/env python3
"""
Test browser connection and launch via WebSocket
"""

import asyncio
import json
import websockets
from datetime import datetime

class BrowserController:
    """Control browser via WebSocket"""
    
    def __init__(self):
        self.ws = None
        self.client_id = None
    
    async def connect(self):
        """Connect to WebSocket"""
        self.ws = await websockets.connect("ws://localhost:9000")
        
        # Client init
        await self.ws.send(json.dumps({
            "type": "client_init",
            "data": {
                "userAgent": "Python Browser Controller",
                "url": "python://browser_test",
                "timestamp": datetime.now().isoformat()
            }
        }))
        
        # Get client ID
        response = await self.ws.recv()
        message = json.loads(response)
        self.client_id = message.get("data", {}).get("clientId")
        print(f"✅ Connected: {self.client_id}")
        return self
    
    async def send_message(self, message_type: str, data: dict = None):
        """Send WebSocket message"""
        message = {
            "type": message_type,
            "timestamp": datetime.now().isoformat(), 
            "clientId": self.client_id
        }
        if data:
            message["data"] = data
        
        await self.ws.send(json.dumps(message))
        response = await self.ws.recv()
        return json.loads(response)
    
    async def get_browser_status(self):
        """Check browser connections"""
        stats = await self.send_message("get_stats")
        browser_data = stats.get("data", {}).get("browserConnections", {})
        print(f"🌐 Browser Status:")
        print(f"  Active connections: {browser_data.get('hasActiveConnections')}")
        print(f"  Connected clients: {browser_data.get('connectedClients', 0)}")
        print(f"  Debug mode: {browser_data.get('debugMode')}")
        return browser_data
    
    async def launch_browser(self):
        """Try to launch browser via execute_command"""
        print("🚀 Attempting to launch browser...")
        try:
            result = await self.send_message("execute_command", {
                "command": "browser-launch",
                "args": {}
            })
            print(f"Browser launch result: {result}")
            return result
        except Exception as e:
            print(f"❌ Browser launch failed: {e}")
            return None
    
    async def browser_navigate(self, url="http://localhost:9000"):
        """Try to navigate browser"""
        print(f"🧭 Navigating to {url}...")
        try:
            result = await self.send_message("execute_command", {
                "command": "browser-navigate", 
                "args": {"url": url}
            })
            print(f"Navigation result: {result}")
            return result
        except Exception as e:
            print(f"❌ Navigation failed: {e}")
            return None
    
    async def take_screenshot(self):
        """Try to take screenshot"""
        print("📸 Taking screenshot...")
        try:
            result = await self.send_message("execute_command", {
                "command": "screenshot",
                "args": {}
            })
            print(f"Screenshot result: {result}")
            return result
        except Exception as e:
            print(f"❌ Screenshot failed: {e}")
            return None
    
    async def close(self):
        if self.ws:
            await self.ws.close()

async def test_browser_control():
    """Test full browser control pipeline"""
    print("🌟 Testing Browser Control via WebSocket")
    print("=" * 50)
    
    controller = BrowserController()
    await controller.connect()
    
    # Check current browser status
    await controller.get_browser_status()
    
    # Try launching browser
    await controller.launch_browser()
    
    # Try navigation
    await controller.browser_navigate()
    
    # Wait a moment for browser to load
    await asyncio.sleep(2)
    
    # Check status again
    await controller.get_browser_status()
    
    # Try screenshot
    await controller.take_screenshot()
    
    await controller.close()
    print("\n✅ Browser control test complete!")

if __name__ == "__main__":
    asyncio.run(test_browser_control())