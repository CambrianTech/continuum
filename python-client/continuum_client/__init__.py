"""
Continuum Python Client
Dynamic API with command discovery and JTAG hooks
"""

from .core.client import ContinuumClient
from .core.js_executor import JSExecutor
from .core.dynamic_client import DynamicContinuumClient
from .exceptions.js_errors import JSExecutionError, JSTimeoutError, ConnectionError
from .utils.server_manager import ContinuumServerManager

__version__ = "0.2.0"
__all__ = [
    "ContinuumClient", 
    "DynamicContinuumClient",
    "JSExecutor",
    "JSExecutionError", 
    "JSTimeoutError", 
    "ConnectionError",
    "ContinuumServerManager",
    "connect"  # Convenience function
]

def connect(base_url: str = "http://localhost:9000") -> DynamicContinuumClient:
    """
    Connect to Continuum with dynamic command discovery
    Usage: continuum.health(), continuum.deploy(), continuum.console_logs()
    """
    return DynamicContinuumClient(base_url).connect()