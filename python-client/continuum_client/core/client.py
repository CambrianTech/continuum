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
        
    async def connect(self) -> None:
        """
        Connect to Continuum WebSocket server
        
        Raises:
            ConnectionError: If connection fails
        """
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
            
        except Exception as e:
            raise ConnectionError(f"Failed to connect to {self.url}: {str(e)}")
    
    async def disconnect(self) -> None:
        """Close WebSocket connection"""
        if self.ws:
            await self.ws.close()
            self.connected = False
    
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
                    if data.get('type') == 'js_executed' and self.js:
                        self.js.handle_ws_message(data)
                    
                    # Route command responses to pending promises (elegant pattern)
                    # Handle both command_response and response types for task-based commands
                    if (data.get('type') in ['command_response', 'response']) and hasattr(self, 'pending_commands'):
                        # For task-based commands, we need to match by recent timestamp
                        # Since server doesn't echo commandId for task responses
                        if self.pending_commands and data.get('type') == 'response':
                            # Get most recent pending command (task-based approach)
                            recent_command_id = max(self.pending_commands.keys()) if self.pending_commands else None
                            if recent_command_id:
                                future = self.pending_commands[recent_command_id]
                                if not future.done():
                                    future.set_result(data)
                        else:
                            # Traditional command_response with commandId
                            command_id = data.get('commandId')
                            if command_id and command_id in self.pending_commands:
                                future = self.pending_commands[command_id]
                                if not future.done():
                                    future.set_result(data.get('result', data))
                    
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
        
        # Use task format that server expects (like other command integrations)
        params_str = json.dumps(params or {})
        message = {
            'type': 'task',
            'role': 'system',
            'task': f'[CMD:{command}] {params_str}',
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
        
        if result.get('success'):
            return {
                'success': True,
                'message': 'Screenshot captured via elegant SCREENSHOT command',
                'server_response': result
            }
        else:
            return {
                'success': False,
                'error': f"Screenshot command failed: {result.get('error', 'Unknown error')}",
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