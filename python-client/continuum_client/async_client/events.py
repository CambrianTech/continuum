"""
Event Stream Module
Real-time event streaming for autonomous development
"""

import websockets
import json
import asyncio
from typing import AsyncGenerator, Dict, Any, Optional, Set
from .types import ContinuumEvent, EventType, LogLevel

class EventStream:
    """
    Real-time event streaming from Continuum system
    Provides filtered streams for different event types
    """
    
    def __init__(self, ws_url: str):
        self.ws_url = ws_url
        self._connections: Set[websockets.WebSocketServerProtocol] = set()
    
    async def console_logs(self, level: LogLevel = None) -> AsyncGenerator[ContinuumEvent, None]:
        """Stream real-time console logs"""
        uri = f"{self.ws_url}/console"
        
        try:
            async with websockets.connect(uri) as websocket:
                self._connections.add(websocket)
                
                async for message in websocket:
                    try:
                        data = json.loads(message)
                        event = ContinuumEvent(
                            type=EventType.CONSOLE.value,
                            timestamp=data.get('timestamp', ''),
                            source=data.get('source', 'console'),
                            data=data.get('data', {}),
                            level=data.get('level', 'info')
                        )
                        
                        # Filter by level if specified
                        if level is None or event.level == level.value:
                            yield event
                            
                    except json.JSONDecodeError:
                        continue
                        
        except Exception as e:
            print(f"Console stream error: {e}")
        finally:
            self._connections.discard(websocket)
    
    async def daemon_status(self) -> AsyncGenerator[ContinuumEvent, None]:
        """Stream real-time daemon status changes"""
        uri = f"{self.ws_url}/daemons"
        
        try:
            async with websockets.connect(uri) as websocket:
                self._connections.add(websocket)
                
                async for message in websocket:
                    try:
                        data = json.loads(message)
                        event = ContinuumEvent(
                            type=EventType.DAEMON.value,
                            timestamp=data.get('timestamp', ''),
                            source=data.get('daemon', 'unknown'),
                            data=data.get('data', {}),
                            level=data.get('level', 'info')
                        )
                        yield event
                        
                    except json.JSONDecodeError:
                        continue
                        
        except Exception as e:
            print(f"Daemon stream error: {e}")
        finally:
            self._connections.discard(websocket)
    
    async def command_execution(self) -> AsyncGenerator[ContinuumEvent, None]:
        """Stream real-time command execution events"""
        uri = f"{self.ws_url}/commands"
        
        try:
            async with websockets.connect(uri) as websocket:
                self._connections.add(websocket)
                
                async for message in websocket:
                    try:
                        data = json.loads(message)
                        event = ContinuumEvent(
                            type=EventType.COMMAND.value,
                            timestamp=data.get('timestamp', ''),
                            source=data.get('command', 'unknown'),
                            data=data.get('data', {}),
                            level=data.get('level', 'info')
                        )
                        yield event
                        
                    except json.JSONDecodeError:
                        continue
                        
        except Exception as e:
            print(f"Command stream error: {e}")
        finally:
            self._connections.discard(websocket)
    
    async def browser_events(self) -> AsyncGenerator[ContinuumEvent, None]:
        """Stream real-time browser events (navigation, errors, etc.)"""
        uri = f"{self.ws_url}/browser"
        
        try:
            async with websockets.connect(uri) as websocket:
                self._connections.add(websocket)
                
                async for message in websocket:
                    try:
                        data = json.loads(message)
                        event = ContinuumEvent(
                            type=EventType.BROWSER.value,
                            timestamp=data.get('timestamp', ''),
                            source=data.get('source', 'browser'),
                            data=data.get('data', {}),
                            level=data.get('level', 'info')
                        )
                        yield event
                        
                    except json.JSONDecodeError:
                        continue
                        
        except Exception as e:
            print(f"Browser stream error: {e}")
        finally:
            self._connections.discard(websocket)
    
    async def system_status(self) -> AsyncGenerator[ContinuumEvent, None]:
        """Stream system-wide status events"""
        uri = f"{self.ws_url}/system"
        
        try:
            async with websockets.connect(uri) as websocket:
                self._connections.add(websocket)
                
                async for message in websocket:
                    try:
                        data = json.loads(message)
                        event = ContinuumEvent(
                            type=EventType.SYSTEM.value,
                            timestamp=data.get('timestamp', ''),
                            source=data.get('source', 'system'),
                            data=data.get('data', {}),
                            level=data.get('level', 'info')
                        )
                        yield event
                        
                    except json.JSONDecodeError:
                        continue
                        
        except Exception as e:
            print(f"System stream error: {e}")
        finally:
            self._connections.discard(websocket)
    
    async def filtered_stream(self, 
                            event_types: Set[EventType] = None,
                            levels: Set[LogLevel] = None,
                            sources: Set[str] = None) -> AsyncGenerator[ContinuumEvent, None]:
        """
        Multi-filtered event stream
        Combines all event types with flexible filtering
        """
        
        # Create tasks for all requested event streams
        tasks = []
        
        if event_types is None or EventType.CONSOLE in event_types:
            tasks.append(self.console_logs())
        if event_types is None or EventType.DAEMON in event_types:
            tasks.append(self.daemon_status())
        if event_types is None or EventType.COMMAND in event_types:
            tasks.append(self.command_execution())
        if event_types is None or EventType.BROWSER in event_types:
            tasks.append(self.browser_events())
        if event_types is None or EventType.SYSTEM in event_types:
            tasks.append(self.system_status())
        
        # Merge streams with filtering
        async def merge_streams():
            async for stream in asyncio.as_completed(tasks):
                async for event in await stream:
                    # Apply filters
                    if levels and LogLevel(event.level) not in levels:
                        continue
                    if sources and event.source not in sources:
                        continue
                    
                    yield event
        
        async for event in merge_streams():
            yield event
    
    async def close(self):
        """Close all WebSocket connections"""
        for websocket in self._connections.copy():
            await websocket.close()
        self._connections.clear()

# Convenience decorators
def event_handler(event_type: EventType, level: LogLevel = None):
    """Decorator for event handler methods"""
    def decorator(func):
        func._event_type = event_type
        func._event_level = level
        return func
    return decorator

class EventProcessor:
    """
    Higher-level event processor with handler registration
    Enables clean event-driven programming patterns
    """
    
    def __init__(self, event_stream: EventStream):
        self.event_stream = event_stream
        self._handlers: Dict[str, list] = {}
    
    def on(self, event_type: EventType, level: LogLevel = None):
        """Register event handler decorator"""
        def decorator(func):
            key = f"{event_type.value}:{level.value if level else 'all'}"
            if key not in self._handlers:
                self._handlers[key] = []
            self._handlers[key].append(func)
            return func
        return decorator
    
    async def start_processing(self):
        """Start event processing loop"""
        async for event in self.event_stream.filtered_stream():
            # Find matching handlers
            handlers = []
            
            # Exact match
            key = f"{event.type}:{event.level}"
            handlers.extend(self._handlers.get(key, []))
            
            # Type-only match
            key = f"{event.type}:all"
            handlers.extend(self._handlers.get(key, []))
            
            # Execute handlers
            for handler in handlers:
                try:
                    if asyncio.iscoroutinefunction(handler):
                        await handler(event)
                    else:
                        handler(event)
                except Exception as e:
                    print(f"Event handler error: {e}")