"""
Browser Adapters for DevTools Log Monitor
Support for multiple browser types with unified interface
"""

import asyncio
import json
import websockets
import requests
from abc import ABC, abstractmethod
from typing import Dict, List, Optional, Tuple
from datetime import datetime

class BrowserAdapter(ABC):
    """Abstract base class for browser adapters"""
    
    def __init__(self, port: int, target_url: str = "localhost:9000"):
        self.port = port
        self.target_url = target_url
        self.ws = None
        self.connected = False
        self.message_id = 1
    
    @abstractmethod
    async def discover_targets(self) -> List[Dict]:
        """Discover available browser targets"""
        pass
    
    @abstractmethod
    async def connect_to_target(self, target: Dict) -> bool:
        """Connect to specific target"""
        pass
    
    @abstractmethod
    async def enable_logging(self):
        """Enable logging domains"""
        pass
    
    @abstractmethod
    def process_message(self, data: Dict) -> Optional[Dict]:
        """Process browser-specific message format"""
        pass
    
    async def send_command(self, method: str, params: Dict = None) -> Dict:
        """Send command to browser"""
        if not self.connected or not self.ws:
            raise Exception("Not connected to browser")
        
        message = {
            "id": self.message_id,
            "method": method,
            "params": params or {}
        }
        self.message_id += 1
        
        await self.ws.send(json.dumps(message))
        return message
    
    async def disconnect(self):
        """Disconnect from browser"""
        if self.ws:
            await self.ws.close()
            self.connected = False

class ChromeAdapter(BrowserAdapter):
    """Chrome/Chromium DevTools Protocol adapter"""
    
    def __init__(self, port: int = 9222, target_url: str = "localhost:9000"):
        super().__init__(port, target_url)
        self.browser_type = "chrome"
    
    async def discover_targets(self) -> List[Dict]:
        """Discover Chrome DevTools targets"""
        try:
            response = requests.get(f"http://localhost:{self.port}/json", timeout=5)
            targets = response.json()
            
            # Filter for relevant targets
            continuum_targets = []
            for target in targets:
                if (target.get('type') == 'page' and 
                    (self.target_url in target.get('url', '') or 
                     'continuum' in target.get('title', '').lower())):
                    continuum_targets.append({
                        'id': target.get('id'),
                        'title': target.get('title'),
                        'url': target.get('url'),
                        'webSocketDebuggerUrl': target.get('webSocketDebuggerUrl'),
                        'browser': 'chrome'
                    })
            
            return continuum_targets
        except Exception as e:
            print(f"🔌 Chrome: Discovery failed: {e}")
            return []
    
    async def connect_to_target(self, target: Dict) -> bool:
        """Connect to Chrome target"""
        try:
            self.ws = await websockets.connect(target['webSocketDebuggerUrl'])
            self.connected = True
            await self.enable_logging()
            return True
        except Exception as e:
            print(f"🔌 Chrome: Connection failed: {e}")
            return False
    
    async def enable_logging(self):
        """Enable Chrome DevTools logging domains"""
        await self.send_command("Log.enable")
        await self.send_command("Runtime.enable")
        await self.send_command("Network.enable")
    
    def process_message(self, data: Dict) -> Optional[Dict]:
        """Process Chrome DevTools message"""
        method = data.get('method')
        params = data.get('params', {})
        
        if method == 'Log.entryAdded':
            entry = params['entry']
            return {
                'type': 'log',
                'browser': 'chrome',
                'level': entry.get('level', 'info'),
                'text': entry.get('text', ''),
                'source': entry.get('source', 'unknown'),
                'timestamp': datetime.fromtimestamp(entry.get('timestamp', 0) / 1000).isoformat(),
                'raw': entry
            }
        
        elif method == 'Runtime.consoleAPICalled':
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
                'type': 'console',
                'browser': 'chrome',
                'level': params.get('type', 'log'),
                'text': ' '.join(text_parts),
                'source': 'console-api',
                'timestamp': datetime.fromtimestamp(params.get('timestamp', 0) / 1000).isoformat(),
                'raw': params
            }
        
        elif method in ['Network.webSocketFrameReceived', 'Network.webSocketFrameSent']:
            direction = 'received' if 'Received' in method else 'sent'
            response = params.get('response', {})
            return {
                'type': 'websocket',
                'browser': 'chrome',
                'direction': direction,
                'request_id': params.get('requestId'),
                'payload': response.get('payloadData', ''),
                'timestamp': datetime.now().isoformat(),
                'raw': params
            }
        
        return None

class SafariAdapter(BrowserAdapter):
    """Safari WebKit Remote Debugging adapter"""
    
    def __init__(self, port: int = 9999, target_url: str = "localhost:9000"):
        super().__init__(port, target_url)
        self.browser_type = "safari"
    
    async def discover_targets(self) -> List[Dict]:
        """Discover Safari targets"""
        try:
            response = requests.get(f"http://localhost:{self.port}/json", timeout=5)
            targets = response.json()
            
            safari_targets = []
            for target in targets:
                if (target.get('type') == 'page' and 
                    (self.target_url in target.get('url', '') or
                     'continuum' in target.get('title', '').lower())):
                    safari_targets.append({
                        'id': target.get('id'),
                        'title': target.get('title'),
                        'url': target.get('url'),
                        'webSocketDebuggerUrl': target.get('webSocketDebuggerUrl'),
                        'browser': 'safari'
                    })
            
            return safari_targets
        except Exception as e:
            print(f"🔌 Safari: Discovery failed: {e}")
            return []
    
    async def connect_to_target(self, target: Dict) -> bool:
        """Connect to Safari target"""
        try:
            self.ws = await websockets.connect(target['webSocketDebuggerUrl'])
            self.connected = True
            await self.enable_logging()
            return True
        except Exception as e:
            print(f"🔌 Safari: Connection failed: {e}")
            return False
    
    async def enable_logging(self):
        """Enable Safari debugging domains"""
        await self.send_command("Console.enable")
        await self.send_command("Runtime.enable")
        await self.send_command("Page.enable")
    
    def process_message(self, data: Dict) -> Optional[Dict]:
        """Process Safari WebKit message"""
        method = data.get('method')
        params = data.get('params', {})
        
        if method == 'Console.messageAdded':
            message = params.get('message', {})
            return {
                'type': 'console',
                'browser': 'safari',
                'level': message.get('level', 'log'),
                'text': message.get('text', ''),
                'source': 'webkit-console',
                'timestamp': datetime.now().isoformat(),
                'raw': params
            }
        
        elif method == 'Runtime.consoleAPICalled':
            args = params.get('args', [])
            text_parts = [str(arg.get('value', '[object]')) for arg in args]
            
            return {
                'type': 'console',
                'browser': 'safari', 
                'level': params.get('type', 'log'),
                'text': ' '.join(text_parts),
                'source': 'webkit-api',
                'timestamp': datetime.now().isoformat(),
                'raw': params
            }
        
        return None

class FirefoxAdapter(BrowserAdapter):
    """Firefox Remote Debugging adapter"""
    
    def __init__(self, port: int = 6000, target_url: str = "localhost:9000"):
        super().__init__(port, target_url)
        self.browser_type = "firefox"
    
    async def discover_targets(self) -> List[Dict]:
        """Discover Firefox targets"""
        try:
            # Firefox uses different discovery mechanism
            response = requests.get(f"http://localhost:{self.port}/json/list", timeout=5)
            targets = response.json()
            
            firefox_targets = []
            for target in targets:
                if (target.get('type') == 'tab' and 
                    (self.target_url in target.get('url', '') or
                     'continuum' in target.get('title', '').lower())):
                    firefox_targets.append({
                        'id': target.get('actor'),
                        'title': target.get('title'),
                        'url': target.get('url'),
                        'webSocketDebuggerUrl': target.get('webSocketDebuggerURL'),
                        'browser': 'firefox'
                    })
            
            return firefox_targets
        except Exception as e:
            print(f"🔌 Firefox: Discovery failed: {e}")
            return []
    
    async def connect_to_target(self, target: Dict) -> bool:
        """Connect to Firefox target"""
        try:
            self.ws = await websockets.connect(target['webSocketDebuggerUrl'])
            self.connected = True
            await self.enable_logging()
            return True
        except Exception as e:
            print(f"🔌 Firefox: Connection failed: {e}")
            return False
    
    async def enable_logging(self):
        """Enable Firefox debugging domains"""
        await self.send_command("webconsole.startListeners", {"listeners": ["ConsoleAPI"]})
        await self.send_command("runtime.enable")
    
    def process_message(self, data: Dict) -> Optional[Dict]:
        """Process Firefox RDP message"""
        msg_type = data.get('type')
        
        if msg_type == 'consoleAPICall':
            return {
                'type': 'console',
                'browser': 'firefox',
                'level': data.get('level', 'log'),
                'text': ' '.join(str(arg) for arg in data.get('arguments', [])),
                'source': 'firefox-console',
                'timestamp': datetime.now().isoformat(),
                'raw': data
            }
        
        return None

class EdgeAdapter(ChromeAdapter):
    """Microsoft Edge adapter (uses Chrome DevTools Protocol)"""
    
    def __init__(self, port: int = 9223, target_url: str = "localhost:9000"):
        super().__init__(port, target_url)
        self.browser_type = "edge"
    
    def process_message(self, data: Dict) -> Optional[Dict]:
        """Process Edge message (same as Chrome but mark as Edge)"""
        result = super().process_message(data)
        if result:
            result['browser'] = 'edge'
        return result

class OperaAdapter(ChromeAdapter):
    """Opera adapter (uses Chrome DevTools Protocol - Chromium based)"""
    
    def __init__(self, port: int = 9222, target_url: str = "localhost:9000"):
        super().__init__(port, target_url)
        self.browser_type = "opera"
    
    def process_message(self, data: Dict) -> Optional[Dict]:
        """Process Opera message (same as Chrome but mark as Opera)"""
        result = super().process_message(data)
        if result:
            result['browser'] = 'opera'
        return result

# Browser adapter registry
BROWSER_ADAPTERS = {
    'chrome': ChromeAdapter,
    'chromium': ChromeAdapter,
    'safari': SafariAdapter,
    'webkit': SafariAdapter,
    'firefox': FirefoxAdapter,
    'edge': EdgeAdapter,
    'opera': OperaAdapter
}

# Default ports for each browser
DEFAULT_PORTS = {
    'chrome': 9222,
    'chromium': 9222,
    'safari': 9999,
    'webkit': 9999,
    'firefox': 6000,
    'edge': 9223,
    'opera': 9222
}

def create_adapter(browser_type: str, port: Optional[int] = None, target_url: str = "localhost:9000") -> BrowserAdapter:
    """Factory function to create browser adapter"""
    browser_type = browser_type.lower()
    
    if browser_type not in BROWSER_ADAPTERS:
        raise ValueError(f"Unsupported browser: {browser_type}. Supported: {list(BROWSER_ADAPTERS.keys())}")
    
    adapter_class = BROWSER_ADAPTERS[browser_type]
    adapter_port = port or DEFAULT_PORTS[browser_type]
    
    return adapter_class(adapter_port, target_url)

async def auto_detect_browsers(target_url: str = "localhost:9000") -> List[Tuple[str, BrowserAdapter]]:
    """Auto-detect available browsers with Continuum targets"""
    available = []
    
    for browser_type in BROWSER_ADAPTERS.keys():
        try:
            adapter = create_adapter(browser_type, target_url=target_url)
            targets = await adapter.discover_targets()
            
            if targets:
                print(f"🔌 Found {browser_type}: {len(targets)} target(s)")
                available.append((browser_type, adapter))
            
        except Exception as e:
            print(f"🔌 {browser_type} not available: {e}")
    
    return available