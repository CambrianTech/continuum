"""
WebSocket Connection Management
==============================

Core WebSocket connection handling for Continuum communication.
"""

import asyncio
import json
import websockets


class WebSocketConnection:
    """
    Core WebSocket Connection Management
    
    Handles low-level WebSocket connection, message sending/receiving,
    and connection lifecycle management for Continuum communication.
    
    Features:
    - Automatic connection establishment
    - JSON message serialization/deserialization  
    - Connection state tracking
    - Graceful cleanup and disconnection
    """
    
    def __init__(self, ws_url):
        self.ws_url = ws_url
        self.ws = None
        self.is_connected = False
        self.connection_id = None
        
    async def connect(self):
        """Establish WebSocket connection"""
        try:
            self.ws = await websockets.connect(self.ws_url)
            self.is_connected = True
            return True
        except Exception as e:
            print(f"❌ WebSocket connection failed: {e}")
            return False
            
    async def disconnect(self):
        """Close WebSocket connection"""
        if self.ws:
            await self.ws.close()
            self.is_connected = False
            
    async def send_message(self, message):
        """Send JSON message through WebSocket"""
        if not self.is_connected or not self.ws:
            raise ConnectionError("WebSocket not connected")
        await self.ws.send(json.dumps(message))
        
    async def receive_message(self, timeout=2):
        """Receive and parse JSON message"""
        if not self.is_connected or not self.ws:
            raise ConnectionError("WebSocket not connected")
        response = await asyncio.wait_for(self.ws.recv(), timeout=timeout)
        return json.loads(response)