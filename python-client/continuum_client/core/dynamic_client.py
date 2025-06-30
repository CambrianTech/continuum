"""
Dynamic Continuum Client
Auto-discovers commands and provides JTAG hooks with Python-friendly API
"""

import aiohttp
import asyncio
import json
import subprocess
import os
import websockets
from typing import Any, Dict, Optional, Callable, AsyncGenerator
from pathlib import Path
import threading
import queue

class DynamicContinuumClient:
    """
    Async Python client with dynamic command discovery and event-driven JTAG hooks
    
    Usage:
        # Async/await pattern (recommended)
        continuum = await DynamicContinuumClient().connect()
        
        # Commands discovered automatically:
        health = await continuum.health()
        projects = await continuum.projects_list()
        status = await continuum.daemon_status()
        
        # Event streams:
        async for log in continuum.console_stream():
            print(f"Console: {log}")
        
        # JTAG hooks with promises:
        screenshot = await continuum.browser_screenshot()
        
        # Snake_case converts to kebab-case automatically:
        await continuum.preferences_set(key="theme", value="dark")
        # -> executes "preferences-set" command
        
    Sync wrapper available:
        continuum = DynamicContinuumClient().connect_sync()
        health = continuum.health_sync()
    """
    
    def __init__(self, base_url: str = "http://localhost:9000"):
        self.base_url = base_url
        self.api_url = f"{base_url}/api"
        self.ws_url = f"ws://localhost:9000"
        self._commands_cache = None
        self._connected = False
        self._session = None
        self._event_handlers = {}
        
    async def connect(self) -> 'DynamicContinuumClient':
        """Async connect to Continuum system"""
        self._session = aiohttp.ClientSession()
        
        try:
            # Try to connect to existing system
            async with self._session.get(f"{self.base_url}/health", timeout=5) as response:
                if response.status == 200:
                    print("✅ Connected to running Continuum system")
                    self._connected = True
                    return self
        except (aiohttp.ClientError, asyncio.TimeoutError):
            pass
            
        # Auto-start Continuum if not running
        print("🚀 Continuum not running - starting now...")
        await self._start_continuum_async()
        return self
    
    def connect_sync(self) -> 'DynamicContinuumClient':
        """Sync wrapper for connect()"""
        return asyncio.run(self.connect())
        
    async def _start_continuum_async(self):
        """Connect to Continuum system, auto-start if needed"""
        try:
            # Try to connect to existing system
            response = requests.get(f"{self.base_url}/health", timeout=5)
            if response.ok:
                print("✅ Connected to running Continuum system")
                self._connected = True
                return self
        except requests.exceptions.RequestException:
            pass
            
        # Auto-start Continuum if not running
        print("🚀 Continuum not running - starting now...")
        self._start_continuum()
        return self
    
    def _start_continuum(self):
        """Start Continuum system using the CLI"""
        try:
            # Find continuum executable (go up from python-client to root)
            continuum_root = Path(__file__).parent.parent.parent
            continuum_exe = continuum_root / "continuum"
            
            if not continuum_exe.exists():
                raise FileNotFoundError("Continuum executable not found")
            
            # Use subprocess.Popen for non-blocking start
            process = subprocess.Popen(
                [str(continuum_exe), "start"],
                cwd=str(continuum_root),
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE
            )
            
            # Wait for system to be ready
            import time
            for i in range(30):  # Wait up to 30 seconds
                try:
                    response = requests.get(f"{self.base_url}/health", timeout=2)
                    if response.ok:
                        print("✅ Continuum system ready")
                        self._connected = True
                        return
                except:
                    pass
                time.sleep(1)
                
            raise TimeoutError("Continuum failed to start within 30 seconds")
            
        except Exception as e:
            raise ConnectionError(f"Failed to start Continuum: {e}")
    
    def _discover_commands(self) -> Dict[str, Any]:
        """Discover available commands from running system"""
        if self._commands_cache:
            return self._commands_cache
            
        try:
            response = requests.get(f"{self.api_url}/commands", timeout=10)
            if response.ok:
                commands = response.json()
                self._commands_cache = commands.get('commands', {})
                return self._commands_cache
        except requests.exceptions.RequestException as e:
            print(f"⚠️ Command discovery failed: {e}")
            
        return {}
    
    def __getattr__(self, name: str) -> Callable:
        """
        Dynamic method discovery with Python-friendly naming
        
        Examples:
            continuum.health() -> command: "health"
            continuum.projects_list() -> command: "projects-list" 
            continuum.console_logs() -> JTAG hook: console.logs()
            continuum.browser_screenshot() -> JTAG hook: browser.screenshot()
        """
        
        # Handle JTAG hooks first (more specific)
        if name.startswith('console_'):
            return self._get_console_hook(name)
        elif name.startswith('browser_'):
            return self._get_browser_hook(name)
        elif name.startswith('daemon_'):
            return self._get_daemon_hook(name)
        
        # Convert Python snake_case to command kebab-case
        command_name = name.replace('_', '-')
        
        # Check if it's a discovered command
        commands = self._discover_commands()
        
        # Exact match first
        if command_name in commands:
            return self._create_command_executor(command_name, commands[command_name])
        
        # Partial match (for flexibility)
        for cmd_name in commands:
            if cmd_name.startswith(command_name) or command_name in cmd_name:
                return self._create_command_executor(cmd_name, commands[cmd_name])
        
        # If no command found, still create executor (might be valid)
        return self._create_command_executor(command_name, {})
    
    def _create_command_executor(self, command_name: str, command_info: Dict) -> Callable:
        """Create a command executor function"""
        def command_executor(*args, **kwargs):
            return self._execute_command(command_name, command_info, *args, **kwargs)
        
        # Add docstring from command info
        if command_info.get('description'):
            command_executor.__doc__ = command_info['description']
            
        return command_executor
    
    def _execute_command(self, command: str, command_info: Dict, *args, **kwargs) -> Any:
        """Execute a command via the API"""
        
        # Build payload with proper argument mapping
        payload = {
            'command': command,
            'args': kwargs
        }
        
        # Add positional args if any
        if args:
            payload['positional_args'] = list(args)
        
        try:
            response = requests.post(
                f"{self.api_url}/command", 
                json=payload,
                timeout=30
            )
            
            if response.ok:
                result = response.json()
                if result.get('success'):
                    return result.get('data')
                else:
                    error_msg = result.get('error', 'Unknown error')
                    raise RuntimeError(f"Command '{command}' failed: {error_msg}")
            else:
                raise RuntimeError(f"API error {response.status_code}: {response.text}")
                
        except requests.exceptions.RequestException as e:
            raise ConnectionError(f"Failed to execute command '{command}': {e}")
    
    def _get_console_hook(self, hook_name: str) -> Callable:
        """JTAG console hooks"""
        def console_hook(*args, **kwargs):
            if hook_name == 'console_capture':
                response = requests.get(f"{self.api_url}/console/connect")
                return response.json() if response.ok else None
            elif hook_name == 'console_logs':
                response = requests.get(f"{self.api_url}/console/logs")
                return response.json() if response.ok else []
            elif hook_name == 'console_stream':
                # TODO: WebSocket streaming for real-time logs
                import websocket
                ws = websocket.WebSocket()
                ws.connect(f"{self.ws_url}/console")
                return ws
            else:
                raise AttributeError(f"Unknown console hook: {hook_name}")
        return console_hook
    
    def _get_browser_hook(self, hook_name: str) -> Callable:
        """JTAG browser hooks"""
        def browser_hook(*args, **kwargs):
            if hook_name == 'browser_screenshot':
                response = requests.get(f"{self.api_url}/browser/screenshot")
                return response.content if response.ok else None
            elif hook_name == 'browser_navigate':
                url = args[0] if args else kwargs.get('url')
                if not url:
                    raise ValueError("URL required for browser_navigate")
                response = requests.post(
                    f"{self.api_url}/browser/navigate",
                    json={'url': url}
                )
                return response.json() if response.ok else None
            elif hook_name == 'browser_status':
                response = requests.get(f"{self.api_url}/browser/status")
                return response.json() if response.ok else {}
            else:
                raise AttributeError(f"Unknown browser hook: {hook_name}")
        return browser_hook
    
    def _get_daemon_hook(self, hook_name: str) -> Callable:
        """JTAG daemon hooks"""
        def daemon_hook(*args, **kwargs):
            if hook_name == 'daemon_list':
                response = requests.get(f"{self.api_url}/daemons")
                return response.json() if response.ok else []
            elif hook_name == 'daemon_status':
                daemon_name = args[0] if args else kwargs.get('daemon', 'all')
                if daemon_name == 'all':
                    response = requests.get(f"{self.api_url}/daemons/status")
                else:
                    response = requests.get(f"{self.api_url}/daemon/{daemon_name}/status")
                return response.json() if response.ok else {}
            elif hook_name == 'daemon_restart':
                daemon_name = args[0] if args else kwargs.get('daemon')
                if not daemon_name:
                    raise ValueError("Daemon name required for daemon_restart")
                response = requests.post(f"{self.api_url}/daemon/{daemon_name}/restart")
                return response.json() if response.ok else None
            else:
                raise AttributeError(f"Unknown daemon hook: {hook_name}")
        return daemon_hook
    
    # Convenience methods with proper typing
    def list_commands(self) -> Dict[str, Any]:
        """List all available commands with descriptions"""
        return self._discover_commands()
    
    def status(self) -> Dict[str, Any]:
        """Get system status"""
        try:
            response = requests.get(f"{self.base_url}/health")
            return response.json() if response.ok else {'running': False}
        except:
            return {'running': False}
    
    def is_connected(self) -> bool:
        """Check if connected to Continuum"""
        return self._connected
    
    def refresh_commands(self) -> Dict[str, Any]:
        """Force refresh of command cache"""
        self._commands_cache = None
        return self._discover_commands()

# Example usage for testing
if __name__ == "__main__":
    continuum = DynamicContinuumClient().connect()
    
    print("Available commands:", list(continuum.list_commands().keys()))
    
    # Dynamic command execution
    try:
        health = continuum.health()
        print("Health check:", health)
    except Exception as e:
        print("Health check failed:", e)
    
    # JTAG hooks
    try:
        daemons = continuum.daemon_status()
        print("Daemon status:", daemons)
    except Exception as e:
        print("Daemon status failed:", e)