"""
Continuum Client
Main client class for connecting to Continuum WebSocket server
"""

import asyncio
import json
import os
import websockets
from typing import Optional, Callable, Dict, Any
from ..exceptions.js_errors import ConnectionError
from .js_executor import JSExecutor

class ContinuumClient:
    """
    Main Continuum client with promise-based JavaScript execution
    Acts as the post office for routing WebSocket messages
    """
    
    def __init__(self, url: Optional[str] = None, timeout: float = 10.0):
        from ..utils.config import get_continuum_ws_url
        # Get URL from environment or use provided/default
        self.url = url or get_continuum_ws_url()
        self.timeout = timeout
        self.ws = None
        self.js = None
        self.connected = False
        self.message_handlers = []
        
        # Universal command interface - initialized after connection
        self.command = None
        
        # DevTools integration - lazy loaded
        self._devtools = None
        
    async def connect(self) -> None:
        """
        Connect to Continuum WebSocket server with auto-healing
        
        Raises:
            ConnectionError: If connection fails after auto-healing attempts
        """
        max_attempts = 2
        
        for attempt in range(max_attempts):
            try:
                self.ws = await websockets.connect(self.url)
                self.connected = True
                
                # Initialize JavaScript executor
                self.js = JSExecutor(self.ws, self.timeout)
                
                # Skip initial status messages
                await self.ws.recv()  # Skip status
                await self.ws.recv()  # Skip banner
                
                # Start message handling loop
                asyncio.create_task(self._message_loop())
                
                # Initialize universal command interface after connection
                from .command_interface import CommandInterface
                self.command = CommandInterface(self)
                
                return  # Success
                
            except Exception as e:
                if attempt == 0 and ("Connect call failed" in str(e) or "Connection refused" in str(e)):
                    print(f"🔧 Connection failed (attempt {attempt + 1}/{max_attempts}), auto-healing...")
                    if await self._auto_heal_server():
                        print("✅ Auto-healing: Server started, retrying connection...")
                        await asyncio.sleep(3)  # Give server time to be ready
                        continue
                    else:
                        print("⚠️ Auto-healing: Could not start server automatically")
                
                if attempt == max_attempts - 1:
                    raise ConnectionError(f"Failed to connect to {self.url}: {str(e)}")
    
    async def _auto_heal_server(self):
        """Auto-heal server connection issues by starting Continuum server"""
        import subprocess
        import time
        
        try:
            # Check if server process is running
            result = subprocess.run(['pgrep', '-f', 'continuum.cjs'], 
                                  capture_output=True, text=True)
            if result.returncode != 0:
                print("🚀 Auto-healing: Starting Continuum server...")
                subprocess.Popen(['node', 'continuum.cjs'], 
                               stdout=subprocess.DEVNULL, 
                               stderr=subprocess.DEVNULL)
                time.sleep(5)  # Give server time to start
                return True
        except Exception as e:
            print(f"⚠️ Auto-healing failed: {e}")
            return False
        
        # Server is running, might be a port/connection issue
        print("🔌 Auto-healing: Server running, connection issue may resolve...")
        return False
    
    async def disconnect(self) -> None:
        """Close WebSocket connection"""
        if self.ws:
            await self.ws.close()
            self.connected = False
            
    @property
    def devtools(self):
        """
        Lazy-loaded DevTools client for browser automation
        
        Usage:
            client = ContinuumClient()
            client.devtools.take_screenshot("test.png")
            client.devtools.execute_script("console.log('Hello')")
        """
        if self._devtools is None:
            from ..devtools import DevToolsClient
            self._devtools = DevToolsClient()
        return self._devtools
    
    async def _message_loop(self):
        """
        Main message handling loop (post office dispatcher)
        Routes incoming messages to appropriate handlers
        """
        try:
            async for message in self.ws:
                try:
                    data = json.loads(message)
                    
                    # Route to JavaScript executor if it's a js_executed response
                    if isinstance(data, dict) and data.get('type') == 'js_executed' and self.js:
                        self.js.handle_ws_message(data)
                    
                    # Route command responses to pending promises (elegant pattern)
                    # Handle bus_command_execution, command_response and response types
                    if hasattr(self, 'pending_commands') and self.pending_commands:
                        # Handle BusCommand results (the actual command execution results)
                        if isinstance(data, dict) and data.get('type') == 'bus_command_execution' and data.get('role') == 'BusCommand':
                            # Get most recent pending command for bus command results
                            recent_command_id = max(self.pending_commands.keys()) if self.pending_commands else None
                            if recent_command_id:
                                future = self.pending_commands[recent_command_id]
                                if not future.done():
                                    # Return the actual command result
                                    result = data.get('result', {})
                                    if isinstance(result, dict) and 'result' in result:
                                        future.set_result(result['result'])
                                    else:
                                        future.set_result(result)
                        
                        # Handle traditional command responses with commandId
                        elif isinstance(data, dict) and data.get('type') == 'command_response':
                            command_id = data.get('commandId')
                            if command_id and command_id in self.pending_commands:
                                future = self.pending_commands[command_id]
                                if not future.done():
                                    future.set_result(data.get('result', data))
                        
                        # Ignore regular AI responses (type: 'response') unless they're the only thing pending
                        elif isinstance(data, dict) and data.get('type') == 'response' and data.get('agent') != 'BusCommand':
                            # Only catch AI responses if we have no other command pending for more than 15 seconds
                            oldest_command_time = min(float(cmd_id.split('_')[-1]) for cmd_id in self.pending_commands.keys()) if self.pending_commands else 0
                            current_time = asyncio.get_event_loop().time()
                            if current_time - oldest_command_time > 15:
                                # Command likely failed, return AI response as fallback
                                recent_command_id = max(self.pending_commands.keys()) if self.pending_commands else None
                                if recent_command_id:
                                    future = self.pending_commands[recent_command_id]
                                    if not future.done():
                                        future.set_result(data)
                    
                    # Route to registered message handlers
                    for handler in self.message_handlers:
                        try:
                            await handler(data)
                        except Exception as e:
                            print(f"Message handler error: {e}")
                            
                except json.JSONDecodeError:
                    print(f"Invalid JSON received: {message}")
                    
        except websockets.exceptions.ConnectionClosed:
            self.connected = False
        except Exception as e:
            print(f"Message loop error: {e}")
            self.connected = False
    
    def add_message_handler(self, handler: Callable[[Dict], Any]):
        """Add a custom message handler"""
        self.message_handlers.append(handler)
    
    async def send_command(self, command: str, params: Dict = None, timeout: float = 10.0) -> Dict:
        """
        Send command to Continuum server using elegant promise-based protocol
        
        Args:
            command: Command name (e.g., 'SCREENSHOT')
            params: Command parameters dict
            timeout: Command timeout in seconds
            
        Returns:
            Command result dict (promise-like interface)
        """
        if not self.connected:
            return {'success': False, 'error': 'Not connected to server'}
            
        # Create promise-like future for command response
        future = asyncio.Future()
        command_id = f"{command}_{asyncio.get_event_loop().time()}"
        
        # Store pending command (like js_executor does)
        if not hasattr(self, 'pending_commands'):
            self.pending_commands = {}
        self.pending_commands[command_id] = future
        
        # Send task with clean CLI syntax (no [CMD: prefix)
        params_str = json.dumps(params or {})
        message = {
            'type': 'task',
            'role': 'system',
            'task': f'{command.lower()} {params_str}',
            'commandId': command_id
        }
        
        try:
            await self.ws.send(json.dumps(message))
            
            # Wait for response with timeout (promise-like behavior)
            result = await asyncio.wait_for(future, timeout=timeout)
            
            # Clean up pending command
            self.pending_commands.pop(command_id, None)
            
            return result
            
        except asyncio.TimeoutError:
            self.pending_commands.pop(command_id, None)
            return {'success': False, 'error': f'{command} command timed out after {timeout}s'}
        except Exception as e:
            self.pending_commands.pop(command_id, None)
            return {'success': False, 'error': str(e)}
    
    async def screenshot(self, selector='body', name_prefix='screenshot', scale=1.0) -> Dict:
        """
        Elegant screenshot capture using Continuum SCREENSHOT command
        
        Args:
            selector: CSS selector for target element (default: 'body' for full page)
            name_prefix: Prefix for filename (default: 'screenshot')
            scale: Scale factor for capture (default: 1.0)
            
        Returns:
            Dict with success status, filename, and server response
        """
        params = {
            'selector': selector,
            'name_prefix': name_prefix,
            'scale': scale,
            'source': 'python_client'
        }
        
        result = await self.send_command('SCREENSHOT', params)
        
        # Handle both string and dict responses
        if isinstance(result, str):
            # String response - assume success
            return {
                'success': True,
                'message': result,
                'server_response': result
            }
        elif isinstance(result, dict) and result.get('success'):
            return {
                'success': True,
                'message': 'Screenshot captured via elegant SCREENSHOT command',
                'server_response': result
            }
        else:
            # Failed dict response or unknown format
            error_msg = result.get('error', 'Unknown error') if isinstance(result, dict) else str(result)
            return {
                'success': False,
                'error': f"Screenshot command failed: {error_msg}",
                'server_response': result
            }

    async def register_agent(self, agent_info: Dict) -> None:
        """
        Register as an agent with the Continuum server
        
        Args:
            agent_info: Agent information dict
        """
        message = {
            'type': 'agent_register',
            'agentInfo': agent_info
        }
        await self.ws.send(json.dumps(message))
    
    async def send_message(self, message: str, agent_id: str, target_agent: str = 'auto') -> None:
        """
        Send a chat message as an agent
        
        Args:
            message: Message text
            agent_id: Sending agent ID
            target_agent: Target agent (default: 'auto')
        """
        msg = {
            'type': 'agent_message',
            'agentId': agent_id,
            'message': message,
            'targetAgent': target_agent,
            'timestamp': asyncio.get_event_loop().time()
        }
        await self.ws.send(json.dumps(msg))
    
    async def __aenter__(self):
        """Async context manager entry"""
        await self.connect()
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit"""
        await self.disconnect()