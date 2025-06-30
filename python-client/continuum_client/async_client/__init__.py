"""
Continuum Async Client
Modern async/await Python client with dynamic command discovery
"""

from .client import AsyncContinuumClient
from .discovery import CommandDiscovery
from .hooks import JTAGHooks
from .events import EventStream
from .types import ContinuumCommand, ContinuumEvent, ContinuumStatus

__all__ = [
    'AsyncContinuumClient',
    'CommandDiscovery', 
    'JTAGHooks',
    'EventStream',
    'ContinuumCommand',
    'ContinuumEvent',
    'ContinuumStatus',
    'connect'
]

async def connect(base_url: str = "http://localhost:9000") -> AsyncContinuumClient:
    """
    Connect to Continuum with full async support
    
    Usage:
        continuum = await connect()
        health = await continuum.health()
        async for log in continuum.console_stream():
            print(log)
    """
    client = AsyncContinuumClient(base_url)
    return await client.connect()