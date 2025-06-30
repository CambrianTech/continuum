#!/usr/bin/env python3
"""
WebSocket Portal - Pure WebSocket Client Mirroring Browser Architecture
Uses the real Continuum WebSocket protocol, not HTTP APIs
"""

import asyncio
import json
import time
import websockets
import multiprocessing as mp
from datetime import datetime
from pathlib import Path
from typing import Dict, Any, Optional, List, Callable
import uuid
import logging

# ============================================================================
# WEBSOCKET CLIENT - Mirrors continuum-browser.ts Architecture
# ============================================================================

class ContinuumWebSocketClient:
    """
    Pure WebSocket client mirroring the browser ContinuumBrowserAPI
    Uses the same protocol and message patterns
    """
    
    def __init__(self, ws_url: str = "ws://localhost:9000"):
        self.ws_url = ws_url
        self.ws = None
        self.connection_state = "disconnected"
        self.event_handlers = {}
        self.message_queue = []
        self.client_id = None
        self.reconnect_attempts = 0
        self.max_reconnect_attempts = 5
        self.reconnect_delay = 1.0
        
        # Message handling
        self._pending_responses = {}
        self._message_id_counter = 0
        
        print("🌐 Continuum WebSocket Client: Initializing...")
    
    async def connect(self):
        """Connect using WebSocket protocol exactly like browser client"""
        if self.connection_state in ["connecting", "connected"]:
            return self
        
        print(f"🌐 Continuum API: Connecting to {self.ws_url}...")
        self.connection_state = "connecting"
        self.emit("continuum:connecting")
        
        try:
            # Connect with timeout like browser client
            self.ws = await asyncio.wait_for(
                websockets.connect(self.ws_url),
                timeout=5.0
            )
            
            print("🌐 Continuum API: WebSocket connection established")
            self.connection_state = "connected"
            self.reconnect_attempts = 0
            
            # Send client init message like browser client
            await self.send_message({
                "type": "client_init",
                "data": {
                    "userAgent": "Python WebSocket Portal",
                    "url": "python://websocket_portal",
                    "timestamp": datetime.now().isoformat()
                }
            })
            
            self.emit("continuum:connected")
            
            # Start message handling
            asyncio.create_task(self._handle_messages())
            
            return self
            
        except asyncio.TimeoutError:
            print("🌐 Continuum API: Connection timeout")
            self.connection_state = "error"
            self.emit("continuum:error", {"error": "Connection timeout"})
            raise ConnectionError("WebSocket connection timeout")
        except Exception as e:
            print(f"🌐 Continuum API: Connection failed: {e}")
            self.connection_state = "error"
            raise ConnectionError(f"WebSocket connection failed: {e}")
    
    async def _handle_messages(self):
        """Handle incoming WebSocket messages"""
        try:
            async for message in self.ws:
                try:
                    data = json.loads(message)
                    await self._process_message(data)
                except json.JSONDecodeError:
                    print(f"⚠️ Invalid JSON message: {message}")
                except Exception as e:
                    print(f"⚠️ Message handling error: {e}")
        except websockets.exceptions.ConnectionClosed:
            print("🌐 Continuum API: Connection closed")
            self.connection_state = "disconnected"
            self.emit("continuum:disconnected")
            await self._attempt_reconnect()
    
    async def _process_message(self, data: Dict[str, Any]):
        """Process incoming message from server"""
        message_type = data.get("type")
        
        # Handle response to pending requests
        if "id" in data and data["id"] in self._pending_responses:
            future = self._pending_responses.pop(data["id"])
            if not future.cancelled():
                future.set_result(data)
            return
        
        # Handle server events
        if message_type == "client_registered":
            self.client_id = data.get("clientId")
            print(f"🌐 Client registered with ID: {self.client_id}")
        elif message_type == "pong":
            print("🏓 Pong received")
        elif message_type == "console_log":
            self.emit("console", data.get("data", {}))
        elif message_type == "daemon_status":
            self.emit("daemon_status", data.get("data", {}))
        else:
            # Generic event emission
            self.emit(message_type, data.get("data", {}))
    
    async def send_message(self, message: Dict[str, Any]) -> Dict[str, Any]:
        """Send message and optionally wait for response"""
        if self.connection_state != "connected":
            raise RuntimeError("Not connected to WebSocket")
        
        # Add message ID for response tracking
        message_id = str(uuid.uuid4())
        message["id"] = message_id
        
        # Send message
        await self.ws.send(json.dumps(message))
        
        # If this is a request that expects a response
        if message.get("type") in ["execute_command", "get_stats", "get_capabilities"]:
            # Wait for response
            future = asyncio.Future()
            self._pending_responses[message_id] = future
            
            try:
                response = await asyncio.wait_for(future, timeout=30.0)
                return response
            except asyncio.TimeoutError:
                self._pending_responses.pop(message_id, None)
                raise TimeoutError(f"No response for message: {message}")
        
        return {"success": True}
    
    # ========== DYNAMIC COMMAND INTERFACE ==========
    
    async def execute_command(self, command: str, **kwargs) -> Any:
        """Execute any command via WebSocket"""
        response = await self.send_message({
            "type": "execute_command",
            "command": command,
            "args": kwargs
        })
        
        if response.get("success"):
            return response.get("data")
        else:
            raise RuntimeError(f"Command '{command}' failed: {response.get('error')}")
    
    def __getattr__(self, name: str):
        """Dynamic method creation - client.health() becomes client.execute_command('health')"""
        async def dynamic_method(**kwargs):
            return await self.execute_command(name.replace('_', '-'), **kwargs)
        return dynamic_method
    
    # ========== SPECIALIZED JTAG METHODS ==========
    
    async def health(self) -> Dict[str, Any]:
        """System health check"""
        return await self.execute_command("health")
    
    async def screenshot(self, description: str = "debug") -> Optional[bytes]:
        """Take screenshot via WebSocket command"""
        try:
            result = await self.execute_command("screenshot", description=description)
            if result and result.get("success") and result.get("data"):
                # Screenshot data should be base64 encoded
                import base64
                return base64.b64decode(result["data"])
            return None
        except Exception as e:
            print(f"📸 Screenshot failed: {e}")
            return None
    
    async def daemon_status(self) -> Dict[str, Any]:
        """Get daemon status via WebSocket"""
        return await self.execute_command("daemon-status")
    
    async def console_logs(self, limit: int = 10) -> List[Dict]:
        """Get console logs via WebSocket"""
        return await self.execute_command("console-logs", limit=limit)
    
    # ========== EVENT SYSTEM ==========
    
    def on(self, event: str, callback: Callable):
        """Register event handler"""
        if event not in self.event_handlers:
            self.event_handlers[event] = []
        self.event_handlers[event].append(callback)
    
    def emit(self, event: str, data: Any = None):
        """Emit event to handlers"""
        if event in self.event_handlers:
            for handler in self.event_handlers[event]:
                try:
                    if asyncio.iscoroutinefunction(handler):
                        asyncio.create_task(handler(data))
                    else:
                        handler(data)
                except Exception as e:
                    print(f"⚠️ Event handler error: {e}")
    
    async def _attempt_reconnect(self):
        """Attempt to reconnect like browser client"""
        if self.reconnect_attempts >= self.max_reconnect_attempts:
            print("🌐 Max reconnection attempts reached")
            return
        
        self.reconnect_attempts += 1
        await asyncio.sleep(self.reconnect_delay)
        
        try:
            await self.connect()
        except Exception as e:
            print(f"🌐 Reconnection failed: {e}")
            await self._attempt_reconnect()
    
    async def close(self):
        """Clean shutdown"""
        if self.ws:
            await self.ws.close()
        self.connection_state = "disconnected"

# ============================================================================
# PROCESS-BASED PORTAL - Uses Processes Like Browser Uses Web Workers
# ============================================================================

class ProcessManager:
    """
    Manages background processes like browser manages web workers
    Each process handles specialized tasks (screenshots, logs, etc.)
    """
    
    def __init__(self):
        self.processes = {}
        self.task_queue = mp.Queue()
        self.result_queue = mp.Queue()
    
    def start_worker(self, worker_name: str, worker_func: Callable):
        """Start background worker process"""
        process = mp.Process(target=worker_func, args=(self.task_queue, self.result_queue))
        process.start()
        self.processes[worker_name] = process
        print(f"🔧 Started worker process: {worker_name}")
    
    async def execute_task(self, worker_name: str, task_data: Dict[str, Any]) -> Any:
        """Execute task in background process"""
        task_id = str(uuid.uuid4())
        task = {
            "id": task_id,
            "worker": worker_name,
            "data": task_data
        }
        
        self.task_queue.put(task)
        
        # Wait for result (with timeout)
        timeout = 30.0
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            try:
                result = self.result_queue.get(timeout=1.0)
                if result["id"] == task_id:
                    return result["data"]
            except:
                pass
        
        raise TimeoutError(f"Task {task_id} timed out")
    
    def shutdown(self):
        """Shutdown all worker processes"""
        for name, process in self.processes.items():
            process.terminate()
            process.join()
            print(f"🔧 Shutdown worker process: {name}")

# ============================================================================
# UNIFIED PORTAL - WebSocket Client + Process Management
# ============================================================================

class WebSocketPortal:
    """
    Unified portal combining WebSocket client with process-based workers
    Mirrors browser architecture: WebSocket + Web Workers = WebSocket + Processes
    """
    
    def __init__(self, ws_url: str = "ws://localhost:9000"):
        self.client = ContinuumWebSocketClient(ws_url)
        self.process_manager = ProcessManager()
        self.session_id = datetime.now().strftime("%Y%m%d_%H%M%S")
        
        print(f"🚀 WebSocket Portal initialized - Session: {self.session_id}")
    
    async def connect(self):
        """Connect WebSocket and start worker processes"""
        await self.client.connect()
        
        # Start specialized worker processes
        # self.process_manager.start_worker("screenshot", screenshot_worker)
        # self.process_manager.start_worker("logs", log_processor_worker)
        
        print("📡 WebSocket Portal fully connected")
        return self
    
    # ========== DELEGATE TO WEBSOCKET CLIENT ==========
    
    def __getattr__(self, name: str):
        """Delegate dynamic methods to WebSocket client"""
        return getattr(self.client, name)
    
    # ========== AI-NATIVE WORKFLOWS ==========
    
    async def debug_workflow(self) -> Dict[str, Any]:
        """Complete debugging workflow using pure WebSocket"""
        print("🔍 Running WebSocket debug workflow...")
        
        # Parallel execution using WebSocket
        health_task = asyncio.create_task(self.client.health())
        daemon_task = asyncio.create_task(self.client.daemon_status())
        screenshot_task = asyncio.create_task(self.client.screenshot("debug_workflow"))
        logs_task = asyncio.create_task(self.client.console_logs(10))
        
        # Wait for all
        health, daemons, screenshot, logs = await asyncio.gather(
            health_task, daemon_task, screenshot_task, logs_task
        )
        
        workflow_result = {
            "timestamp": datetime.now().isoformat(),
            "health": health,
            "daemons": daemons,
            "screenshot_captured": screenshot is not None,
            "logs_count": len(logs) if logs else 0,
            "protocol": "websocket"
        }
        
        print(f"✅ WebSocket debug workflow complete: {workflow_result}")
        return workflow_result
    
    async def close(self):
        """Clean shutdown"""
        await self.client.close()
        self.process_manager.shutdown()
        print("📋 WebSocket Portal closed")

# ============================================================================
# MAIN DEMO - Prove Pure WebSocket Works
# ============================================================================

async def main():
    """Demonstrate pure WebSocket portal"""
    print("🌟 WebSocket Portal - Pure Protocol Implementation")
    print("=" * 60)
    
    # Initialize and connect
    portal = WebSocketPortal()
    await portal.connect()
    
    print("\n🎯 WebSocket Protocol Test:")
    
    # Test basic health via WebSocket
    try:
        health = await portal.health()
        print(f"  ✅ Health via WebSocket: {health}")
    except Exception as e:
        print(f"  ❌ Health failed: {e}")
    
    # Test daemon status via WebSocket
    try:
        daemons = await portal.daemon_status()
        print(f"  🔧 Daemons via WebSocket: {len(daemons.get('router', {}).get('daemons', []))} active")
    except Exception as e:
        print(f"  ❌ Daemon status failed: {e}")
    
    # Test screenshot via WebSocket
    try:
        screenshot = await portal.screenshot("websocket_test")
        print(f"  📸 Screenshot via WebSocket: {'✅ Captured' if screenshot else '❌ Failed'}")
    except Exception as e:
        print(f"  ❌ Screenshot failed: {e}")
    
    # Test debug workflow
    print("\n🤖 AI Debug Workflow:")
    try:
        debug_result = await portal.debug_workflow()
        print(f"  🔍 Workflow complete: {debug_result['protocol']} protocol")
    except Exception as e:
        print(f"  ❌ Debug workflow failed: {e}")
    
    # Clean shutdown
    await portal.close()
    
    print("\n✨ WebSocket Portal demo complete!")
    print("🎯 Pure WebSocket protocol - no HTTP APIs needed!")

if __name__ == "__main__":
    asyncio.run(main())