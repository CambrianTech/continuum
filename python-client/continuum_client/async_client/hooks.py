"""
JTAG Hooks Module
Provides specialized hooks for autonomous development
"""

import aiohttp
import websockets
import json
from typing import Callable, Dict, Any, Optional, AsyncGenerator
from .types import ContinuumEvent

class JTAGHooks:
    """
    Joint Test Action Group hooks for autonomous development
    Provides real-time integration with browser, console, and daemons
    """
    
    def __init__(self, session: aiohttp.ClientSession, api_url: str, ws_url: str):
        self.session = session
        self.api_url = api_url
        self.ws_url = ws_url
    
    # Console JTAG hooks
    def get_console_method(self, method_name: str) -> Callable:
        """Get console-related JTAG method"""
        
        if method_name == 'console_logs':
            return self._console_logs
        elif method_name == 'console_capture':
            return self._console_capture
        elif method_name == 'console_stream':
            return self._console_stream
        elif method_name == 'console_filter':
            return self._console_filter
        else:
            raise AttributeError(f"Unknown console hook: {method_name}")
    
    async def _console_logs(self, limit: int = 100, level: str = "all") -> list:
        """Get recent console logs"""
        params = {'limit': limit}
        if level != "all":
            params['level'] = level
            
        async with self.session.get(f"{self.api_url}/console/logs", params=params) as response:
            if response.status == 200:
                return await response.json()
            return []
    
    async def _console_capture(self) -> Dict[str, Any]:
        """Start console capture session"""
        async with self.session.post(f"{self.api_url}/console/capture") as response:
            if response.status == 200:
                return await response.json()
            return {}
    
    async def _console_stream(self) -> AsyncGenerator[ContinuumEvent, None]:
        """Real-time console log stream"""
        uri = f"{self.ws_url}/console"
        
        try:
            async with websockets.connect(uri) as websocket:
                async for message in websocket:
                    try:
                        data = json.loads(message)
                        yield ContinuumEvent(
                            type="console",
                            timestamp=data.get('timestamp', ''),
                            source=data.get('source', 'unknown'),
                            data=data.get('data', {}),
                            level=data.get('level', 'info')
                        )
                    except json.JSONDecodeError:
                        continue
        except Exception as e:
            print(f"Console stream error: {e}")
    
    async def _console_filter(self, level: str, source: str = None) -> AsyncGenerator[ContinuumEvent, None]:
        """Filtered console stream"""
        async for event in self._console_stream():
            if event.level == level:
                if source is None or event.source == source:
                    yield event
    
    # Browser JTAG hooks
    def get_browser_method(self, method_name: str) -> Callable:
        """Get browser-related JTAG method"""
        
        if method_name == 'browser_screenshot':
            return self._browser_screenshot
        elif method_name == 'browser_navigate':
            return self._browser_navigate
        elif method_name == 'browser_status':
            return self._browser_status
        elif method_name == 'browser_devtools':
            return self._browser_devtools
        elif method_name == 'browser_elements':
            return self._browser_elements
        else:
            raise AttributeError(f"Unknown browser hook: {method_name}")
    
    async def _browser_screenshot(self, format: str = "png") -> Optional[bytes]:
        """Capture browser screenshot"""
        params = {'format': format}
        async with self.session.get(f"{self.api_url}/browser/screenshot", params=params) as response:
            if response.status == 200:
                return await response.read()
            return None
    
    async def _browser_navigate(self, url: str) -> Dict[str, Any]:
        """Navigate browser to URL"""
        payload = {'url': url}
        async with self.session.post(f"{self.api_url}/browser/navigate", json=payload) as response:
            if response.status == 200:
                return await response.json()
            return {}
    
    async def _browser_status(self) -> Dict[str, Any]:
        """Get browser status and metrics"""
        async with self.session.get(f"{self.api_url}/browser/status") as response:
            if response.status == 200:
                return await response.json()
            return {}
    
    async def _browser_devtools(self, command: str, params: Dict[str, Any] = None) -> Dict[str, Any]:
        """Execute DevTools protocol command"""
        payload = {'command': command, 'params': params or {}}
        async with self.session.post(f"{self.api_url}/browser/devtools", json=payload) as response:
            if response.status == 200:
                return await response.json()
            return {}
    
    async def _browser_elements(self, selector: str) -> list:
        """Query browser elements"""
        params = {'selector': selector}
        async with self.session.get(f"{self.api_url}/browser/elements", params=params) as response:
            if response.status == 200:
                return await response.json()
            return []
    
    # Daemon JTAG hooks
    def get_daemon_method(self, method_name: str) -> Callable:
        """Get daemon-related JTAG method"""
        
        if method_name == 'daemon_list':
            return self._daemon_list
        elif method_name == 'daemon_status':
            return self._daemon_status
        elif method_name == 'daemon_restart':
            return self._daemon_restart
        elif method_name == 'daemon_logs':
            return self._daemon_logs
        elif method_name == 'daemon_health':
            return self._daemon_health
        else:
            raise AttributeError(f"Unknown daemon hook: {method_name}")
    
    async def _daemon_list(self) -> list:
        """List all daemons"""
        async with self.session.get(f"{self.api_url}/daemons") as response:
            if response.status == 200:
                return await response.json()
            return []
    
    async def _daemon_status(self, daemon: str = "all") -> Dict[str, Any]:
        """Get daemon status"""
        if daemon == "all":
            endpoint = f"{self.api_url}/daemons/status"
        else:
            endpoint = f"{self.api_url}/daemon/{daemon}/status"
            
        async with self.session.get(endpoint) as response:
            if response.status == 200:
                return await response.json()
            return {}
    
    async def _daemon_restart(self, daemon: str) -> Dict[str, Any]:
        """Restart specific daemon"""
        async with self.session.post(f"{self.api_url}/daemon/{daemon}/restart") as response:
            if response.status == 200:
                return await response.json()
            return {}
    
    async def _daemon_logs(self, daemon: str, limit: int = 50) -> list:
        """Get daemon logs"""
        params = {'limit': limit}
        async with self.session.get(f"{self.api_url}/daemon/{daemon}/logs", params=params) as response:
            if response.status == 200:
                return await response.json()
            return []
    
    async def _daemon_health(self, daemon: str) -> Dict[str, Any]:
        """Get daemon health metrics"""
        async with self.session.get(f"{self.api_url}/daemon/{daemon}/health") as response:
            if response.status == 200:
                return await response.json()
            return {}