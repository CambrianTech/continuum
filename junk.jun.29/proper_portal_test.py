#!/usr/bin/env python3
"""
Proper Portal Test - Let Continuum handle browser launch automatically
No cheating - pure client connection should trigger system startup
"""

import asyncio
import json
import websockets
from datetime import datetime
import time

class ProperContinuumPortal:
    """Portal that relies on Continuum OS to handle everything"""
    
    def __init__(self):
        self.ws = None
        self.client_id = None
        self.session_logs = []
    
    def log(self, message: str, data: dict = None):
        """Session logging like a real AI persona would need"""
        entry = {
            "timestamp": datetime.now().isoformat(),
            "message": message,
            "data": data
        }
        self.session_logs.append(entry)
        print(f"📝 {message}")
        if data:
            print(f"    {data}")
    
    async def connect_as_persona(self):
        """Connect as AI persona - let Continuum handle the rest"""
        self.log("🤖 AI Persona connecting to Continuum OS...")
        
        try:
            self.ws = await websockets.connect("ws://localhost:9000")
            self.log("✅ WebSocket connection established")
            
            # Register as AI persona client
            await self.ws.send(json.dumps({
                "type": "client_init",
                "data": {
                    "userAgent": "AI Persona Portal",
                    "clientType": "ai_persona",
                    "url": "python://ai_persona",
                    "timestamp": datetime.now().isoformat()
                }
            }))
            
            # Wait for system response
            response = await self.ws.recv()
            message = json.loads(response)
            
            if message.get("type") == "connection_confirmed":
                self.client_id = message.get("data", {}).get("clientId")
                browser_state = message.get("data", {}).get("browserState", {})
                
                self.log("🆔 Client registered", {
                    "client_id": self.client_id,
                    "browser_connections": browser_state.get("connectedClients", 0),
                    "has_active_browser": browser_state.get("hasActiveConnections", False)
                })
                
                return self
            else:
                self.log("❌ Unexpected response", message)
                return None
                
        except Exception as e:
            self.log(f"❌ Connection failed: {e}")
            return None
    
    async def wait_for_system_ready(self, timeout: int = 30):
        """Wait for Continuum to set up complete system"""
        self.log("⏳ Waiting for Continuum OS to initialize complete system...")
        
        start_time = time.time()
        while time.time() - start_time < timeout:
            # Check system status
            await self.ws.send(json.dumps({
                "type": "get_stats",
                "timestamp": datetime.now().isoformat(),
                "clientId": self.client_id
            }))
            
            response = await self.ws.recv()
            stats = json.loads(response)
            
            browser_data = stats.get("data", {}).get("browserConnections", {})
            daemon_data = stats.get("data", {}).get("daemonConnector", {})
            
            has_browser = browser_data.get("hasActiveConnections", False)
            daemon_connected = daemon_data.get("connected", False)
            
            self.log("🔍 System status check", {
                "browser_active": has_browser,
                "daemon_connected": daemon_connected,
                "browser_clients": browser_data.get("connectedClients", 0)
            })
            
            if has_browser and daemon_connected:
                self.log("✅ Complete system ready!")
                return True
                
            await asyncio.sleep(2)
        
        self.log("⚠️ System not fully ready within timeout")
        return False
    
    async def test_system_capabilities(self):
        """Test what the properly initialized system can do"""
        self.log("🧪 Testing system capabilities...")
        
        # Test command execution
        try:
            await self.ws.send(json.dumps({
                "type": "execute_command",
                "data": {
                    "command": "health",
                    "args": {}
                },
                "timestamp": datetime.now().isoformat(),
                "clientId": self.client_id
            }))
            
            response = await self.ws.recv()
            result = json.loads(response)
            self.log("✅ Health command executed", result)
            
        except Exception as e:
            self.log(f"❌ Command execution failed: {e}")
    
    async def close_session(self):
        """Close with session summary"""
        if self.ws:
            await self.ws.close()
        
        self.log("📋 Session complete", {
            "total_logs": len(self.session_logs),
            "session_duration": f"{time.time()} seconds"
        })
        
        # Save session logs like a real AI would need
        with open(f"portal_sessions/ai_persona_session_{int(time.time())}.json", "w") as f:
            json.dump(self.session_logs, f, indent=2)

async def test_proper_portal():
    """Test portal the RIGHT way - let Continuum handle everything"""
    print("🌟 Proper AI Persona Portal Test")
    print("=" * 50)
    print("🚫 NO CHEATING - Let Continuum OS handle browser launch")
    print()
    
    portal = ProperContinuumPortal()
    
    # Connect as AI persona
    if not await portal.connect_as_persona():
        print("❌ Failed to connect")
        return
    
    # Wait for Continuum to set up complete system
    system_ready = await portal.wait_for_system_ready()
    
    if system_ready:
        # Test system capabilities
        await portal.test_system_capabilities()
    else:
        portal.log("⚠️ System not fully initialized - checking what's available")
    
    # Close properly
    await portal.close_session()
    
    print("\n✨ Proper portal test complete!")
    print("🎯 This is how an AI persona should connect to Continuum!")

if __name__ == "__main__":
    import os
    os.makedirs("portal_sessions", exist_ok=True)
    asyncio.run(test_proper_portal())