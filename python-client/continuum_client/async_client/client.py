"""
Main async Continuum client
Clean, composable architecture mirroring the browser client
"""

import aiohttp
import asyncio
from typing import Any, Dict, Optional, Callable
from pathlib import Path

from .discovery import CommandDiscovery
from .hooks import JTAGHooks  
from .events import EventStream
from .types import ContinuumStatus, CommandResult

class AsyncContinuumClient:
    """
    Modern async Continuum client with elegant Python patterns
    
    Architecture mirrors the browser client:
    - Clean separation of concerns
    - Composable modules (discovery, hooks, events)
    - Type-safe interfaces
    - Async/await throughout
    """
    
    def __init__(self, base_url: str = "http://localhost:9000"):
        self.base_url = base_url
        self.api_url = f"{base_url}/api"
        self.ws_url = f"ws://localhost:9000"
        
        # Composable modules
        self._session: Optional[aiohttp.ClientSession] = None
        self._discovery: Optional[CommandDiscovery] = None
        self._hooks: Optional[JTAGHooks] = None
        self._events: Optional[EventStream] = None
        
        self._connected = False
    
    async def connect(self) -> 'AsyncContinuumClient':
        """Connect and initialize all modules"""
        self._session = aiohttp.ClientSession()
        
        # Try existing system first
        if await self._check_existing_system():
            print("✅ Connected to running Continuum")
        else:
            print("🚀 Starting Continuum system...")
            await self._start_system()
        
        # Initialize composable modules
        self._discovery = CommandDiscovery(self._session, self.api_url)
        self._hooks = JTAGHooks(self._session, self.api_url, self.ws_url)
        self._events = EventStream(self.ws_url)
        
        self._connected = True
        return self
    
    async def _check_existing_system(self) -> bool:
        """Check if Continuum is already running"""
        try:
            async with self._session.get(f"{self.base_url}/health", timeout=5) as response:
                return response.status == 200
        except (aiohttp.ClientError, asyncio.TimeoutError):
            return False
    
    async def _start_system(self) -> None:
        """Start Continuum system if not running"""
        continuum_root = Path(__file__).parent.parent.parent.parent
        continuum_exe = continuum_root / "continuum"
        
        if not continuum_exe.exists():
            raise FileNotFoundError("Continuum executable not found")
        
        # Non-blocking start
        process = await asyncio.create_subprocess_exec(
            str(continuum_exe), "start",
            cwd=str(continuum_root),
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE
        )
        
        # Wait for system ready
        for _ in range(30):
            if await self._check_existing_system():
                return
            await asyncio.sleep(1)
        
        raise TimeoutError("Continuum failed to start")
    
    def __getattr__(self, name: str) -> Callable:
        """
        Dynamic method discovery with async support
        
        Examples:
            await continuum.health()
            await continuum.projects_list() 
            await continuum.preferences_set(theme="dark")
        """
        if not self._connected:
            raise RuntimeError("Client not connected. Call await client.connect() first")
        
        # JTAG hooks (higher priority)
        if name.startswith('console_'):
            return self._hooks.get_console_method(name)
        elif name.startswith('browser_'):
            return self._hooks.get_browser_method(name)
        elif name.startswith('daemon_'):
            return self._hooks.get_daemon_method(name)
        
        # Dynamic commands
        return self._discovery.get_command_method(name)
    
    # High-level API methods
    async def status(self) -> ContinuumStatus:
        """Get comprehensive system status"""
        async with self._session.get(f"{self.base_url}/health") as response:
            data = await response.json()
            return ContinuumStatus(
                running=response.status == 200,
                daemons=data.get('daemons', {}),
                api_version=data.get('version', 'unknown'),
                uptime=data.get('uptime', 0),
                health=data.get('health', 'unknown')
            )
    
    async def list_commands(self) -> Dict[str, Any]:
        """List all available commands"""
        return await self._discovery.discover_commands()
    
    # Event streaming
    def console_stream(self):
        """Real-time console log stream"""
        return self._events.console_logs()
    
    def daemon_events(self):
        """Real-time daemon status events"""
        return self._events.daemon_status()
    
    def system_events(self):
        """Real-time system events"""
        return self._events.system_status()
    
    # Context manager support
    async def __aenter__(self):
        return await self.connect()
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        await self.close()
    
    async def close(self):
        """Clean shutdown"""
        if self._events:
            await self._events.close()
        if self._session:
            await self._session.close()
        self._connected = False

# Convenience functions
async def health_check(base_url: str = "http://localhost:9000") -> bool:
    """Quick health check without full client"""
    async with aiohttp.ClientSession() as session:
        try:
            async with session.get(f"{base_url}/health", timeout=5) as response:
                return response.status == 200
        except:
            return False

async def quick_command(command: str, args: Dict[str, Any] = None, 
                       base_url: str = "http://localhost:9000") -> CommandResult:
    """Execute single command without persistent client"""
    async with AsyncContinuumClient(base_url) as client:
        method = getattr(client, command.replace('-', '_'))
        return await method(**(args or {}))