"""
DevTools Log Monitor
Immediate solution for browser console log capture with extensibility foundation
"""

import asyncio
import json
import websockets
import requests
from typing import Dict, List, Optional, Callable, Any
from datetime import datetime
import logging

class DevToolsLogMonitor:
    """
    Focused log monitoring solution for immediate needs
    Built with extensibility in mind for future browser automation
    """
    
    def __init__(self, 
                 chrome_port: int = 9222,
                 target_url: str = "localhost:9000",
                 log_callback: Optional[Callable] = None):
        self.chrome_port = chrome_port
        self.target_url = target_url
        self.log_callback = log_callback or self._default_log_callback
        
        # WebSocket connection to Chrome DevTools
        self.ws = None
        self.connected = False
        self.message_id = 1
        
        # Log storage for immediate retrieval
        self.console_logs = []
        self.websocket_frames = []
        self.max_stored_logs = 500
        
        # Extensibility: Plugin system for future features
        self.plugins = {}
        self.event_handlers = {}
        
    async def connect(self) -> bool:
        """Connect to Chrome DevTools for log monitoring"""
        try:
            # Get Chrome DevTools targets
            response = requests.get(f"http://localhost:{self.chrome_port}/json", timeout=5)
            targets = response.json()
            
            # Find Continuum target
            target = None
            for t in targets:
                if (t.get('type') == 'page' and 
                    (self.target_url in t.get('url', '') or 'continuum' in t.get('title', '').lower())):
                    target = t
                    break
            
            if not target:
                print(f"🔌 DevTools: No Continuum target found (looking for {self.target_url})")
                print(f"🔌 DevTools: Available targets: {[t.get('title', 'No title') for t in targets]}")
                return False
            
            print(f"🔌 DevTools: Connecting to: {target.get('title', 'Unknown')}")
            
            # Connect to WebSocket with timeout
            print(f"🔌 DevTools: Connecting to WebSocket: {target['webSocketDebuggerUrl']}")
            self.ws = await asyncio.wait_for(
                websockets.connect(target['webSocketDebuggerUrl']), 
                timeout=10.0
            )
            self.connected = True
            print("🔌 DevTools: WebSocket connected")
            
            # Start message handling loop first
            asyncio.create_task(self._message_loop())
            
            # Enable required domains for logging and screenshots
            print("🔌 DevTools: Enabling domains...")
            try:
                await asyncio.wait_for(self._send_command("Log.enable"), timeout=5.0)
                await asyncio.wait_for(self._send_command("Runtime.enable"), timeout=5.0)
                await asyncio.wait_for(self._send_command("Network.enable"), timeout=5.0)
                await asyncio.wait_for(self._send_command("Page.enable"), timeout=5.0)
                print("🔌 DevTools: All domains enabled")
            except asyncio.TimeoutError:
                print("🔌 DevTools: Warning - Domain enabling timed out, but connection established")
            
            print("🔌 DevTools: Connected and monitoring console logs")
            return True
            
        except Exception as e:
            print(f"🔌 DevTools: Connection failed: {e}")
            return False
    
    async def _send_command(self, method: str, params: Dict = None) -> Dict:
        """Send command to Chrome DevTools and wait for response"""
        if not self.connected or not self.ws:
            raise Exception("Not connected to DevTools")
        
        command_id = self.message_id
        message = {
            "id": command_id,
            "method": method,
            "params": params or {}
        }
        self.message_id += 1
        
        # Store pending command for response matching
        future = asyncio.get_event_loop().create_future()
        if not hasattr(self, 'pending_commands'):
            self.pending_commands = {}
        self.pending_commands[command_id] = future
        
        await self.ws.send(json.dumps(message))
        
        try:
            # Wait for response (30 second timeout)
            response = await asyncio.wait_for(future, timeout=30.0)
            return response
        except asyncio.TimeoutError:
            self.pending_commands.pop(command_id, None)
            raise Exception(f"Command {method} timed out")
        finally:
            self.pending_commands.pop(command_id, None)
    
    async def _message_loop(self):
        """Handle incoming messages from Chrome DevTools"""
        try:
            async for message in self.ws:
                try:
                    data = json.loads(message)
                    await self._handle_message(data)
                except json.JSONDecodeError:
                    print(f"🔌 DevTools: Invalid JSON: {message}")
                except Exception as e:
                    print(f"🔌 DevTools: Message handling error: {e}")
        except websockets.exceptions.ConnectionClosed:
            print("🔌 DevTools: Connection closed")
            self.connected = False
        except Exception as e:
            print(f"🔌 DevTools: Message loop error: {e}")
            self.connected = False
    
    async def _handle_message(self, data: Dict):
        """Process DevTools protocol messages"""
        # Handle command responses first
        if 'id' in data and hasattr(self, 'pending_commands'):
            command_id = data['id']
            if command_id in self.pending_commands:
                future = self.pending_commands[command_id]
                if not future.done():
                    if 'error' in data:
                        future.set_exception(Exception(f"DevTools error: {data['error']}"))
                    else:
                        future.set_result(data)
                return
        
        # Handle event messages
        method = data.get('method')
        params = data.get('params', {})
        
        # Handle console logs
        if method == 'Log.entryAdded':
            log_entry = self._process_log_entry(params['entry'])
            await self._store_and_forward_log(log_entry)
            
        elif method == 'Runtime.consoleAPICalled':
            console_log = self._process_console_api(params)
            await self._store_and_forward_log(console_log)
            
        # Handle WebSocket frames (for future extensibility)
        elif method == 'Network.webSocketFrameReceived':
            frame = self._process_websocket_frame(params, 'received')
            await self._store_websocket_frame(frame)
            
        elif method == 'Network.webSocketFrameSent':
            frame = self._process_websocket_frame(params, 'sent')
            await self._store_websocket_frame(frame)
        
        # Extensibility: Allow plugins to handle other message types
        await self._trigger_plugin_handlers(method, params)
    
    def _process_log_entry(self, entry: Dict) -> Dict:
        """Process Log.entryAdded event"""
        return {
            'type': 'log-entry',
            'level': entry.get('level', 'info'),
            'text': entry.get('text', ''),
            'source': entry.get('source', 'unknown'),
            'timestamp': datetime.fromtimestamp(entry.get('timestamp', 0) / 1000).isoformat(),
            'raw_timestamp': entry.get('timestamp', 0)
        }
    
    def _process_console_api(self, params: Dict) -> Dict:
        """Process Runtime.consoleAPICalled event"""
        args = params.get('args', [])
        text_parts = []
        
        for arg in args:
            if 'value' in arg:
                text_parts.append(str(arg['value']))
            elif 'description' in arg:
                text_parts.append(arg['description'])
            else:
                text_parts.append('[object]')
        
        return {
            'type': 'console-api',
            'level': params.get('type', 'log'),
            'text': ' '.join(text_parts),
            'source': 'console-api',
            'timestamp': datetime.fromtimestamp(params.get('timestamp', 0) / 1000).isoformat(),
            'raw_timestamp': params.get('timestamp', 0),
            'stack_trace': params.get('stackTrace')
        }
    
    def _process_websocket_frame(self, params: Dict, direction: str) -> Dict:
        """Process WebSocket frame events"""
        response = params.get('response', {})
        return {
            'type': 'websocket-frame',
            'direction': direction,
            'request_id': params.get('requestId'),
            'opcode': response.get('opcode'),
            'mask': response.get('mask'),
            'payload': response.get('payloadData', ''),
            'timestamp': datetime.now().isoformat()
        }
    
    async def _store_and_forward_log(self, log_entry: Dict):
        """Store log and forward to callback"""
        # Store for later retrieval
        self.console_logs.append(log_entry)
        if len(self.console_logs) > self.max_stored_logs:
            self.console_logs.pop(0)
        
        # Forward to callback for immediate processing
        try:
            if asyncio.iscoroutinefunction(self.log_callback):
                await self.log_callback(log_entry)
            else:
                self.log_callback(log_entry)
        except Exception as e:
            print(f"🔌 DevTools: Log callback error: {e}")
    
    async def _store_websocket_frame(self, frame: Dict):
        """Store WebSocket frame for analysis"""
        self.websocket_frames.append(frame)
        if len(self.websocket_frames) > self.max_stored_logs:
            self.websocket_frames.pop(0)
    
    def _default_log_callback(self, log_entry: Dict):
        """Default log handler - just print"""
        timestamp = log_entry['timestamp']
        level = log_entry['level'].upper()
        text = log_entry['text']
        print(f"🔌 [{timestamp}] {level}: {text}")
    
    # Extensibility methods for future plugins
    
    def register_plugin(self, name: str, plugin: Any):
        """Register a plugin for extended functionality"""
        self.plugins[name] = plugin
        print(f"🔌 DevTools: Registered plugin: {name}")
    
    def register_event_handler(self, event_type: str, handler: Callable):
        """Register handler for specific DevTools events"""
        if event_type not in self.event_handlers:
            self.event_handlers[event_type] = []
        self.event_handlers[event_type].append(handler)
    
    async def _trigger_plugin_handlers(self, method: str, params: Dict):
        """Allow plugins to handle DevTools events"""
        handlers = self.event_handlers.get(method, [])
        for handler in handlers:
            try:
                if asyncio.iscoroutinefunction(handler):
                    await handler(method, params)
                else:
                    handler(method, params)
            except Exception as e:
                print(f"🔌 DevTools: Plugin handler error: {e}")
    
    # Data retrieval methods for portal integration
    
    def get_recent_logs(self, limit: int = 50, level: Optional[str] = None) -> List[Dict]:
        """Get recent console logs"""
        logs = self.console_logs
        
        if level and level != 'all':
            logs = [log for log in logs if log['level'] == level]
        
        return logs[-limit:] if limit > 0 else logs
    
    def get_websocket_activity(self, limit: int = 100) -> List[Dict]:
        """Get recent WebSocket frames"""
        return self.websocket_frames[-limit:] if limit > 0 else self.websocket_frames
    
    def get_connection_status(self) -> Dict:
        """Get connection status for portal display"""
        return {
            'connected': self.connected,
            'target_url': self.target_url,
            'chrome_port': self.chrome_port,
            'stored_logs': len(self.console_logs),
            'stored_frames': len(self.websocket_frames)
        }
    
    async def disconnect(self):
        """Disconnect from DevTools"""
        if self.ws:
            await self.ws.close()
            self.connected = False
            print("🔌 DevTools: Disconnected")

# Convenience function for immediate use
async def start_log_monitoring(callback: Optional[Callable] = None) -> DevToolsLogMonitor:
    """Start DevTools log monitoring with default settings"""
    monitor = DevToolsLogMonitor(log_callback=callback)
    success = await monitor.connect()
    
    if success:
        print("🔌 DevTools: Log monitoring started")
        return monitor
    else:
        print("🔌 DevTools: Failed to start monitoring")
        return None