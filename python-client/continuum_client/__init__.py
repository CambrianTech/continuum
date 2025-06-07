"""
Continuum Python Client
Promise-based WebSocket library for browser interaction
"""

from .core.client import ContinuumClient
from .core.js_executor import JSExecutor
from .exceptions.js_errors import JSExecutionError, JSTimeoutError, ConnectionError
from .utils.server_manager import ContinuumServerManager

__version__ = "0.1.0"
__all__ = [
    "ContinuumClient", 
    "JSExecutor",
    "JSExecutionError", 
    "JSTimeoutError", 
    "ConnectionError",
    "ContinuumServerManager"
]