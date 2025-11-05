"""
Type definitions for Continuum async client
Clean interfaces matching the TypeScript definitions
"""

from typing import Dict, Any, Optional, List, Union, AsyncGenerator
from dataclasses import dataclass
from enum import Enum

@dataclass
class ContinuumCommand:
    """Command definition discovered from system"""
    name: str
    description: str
    parameters: Dict[str, Any]
    category: str
    examples: List[str]

@dataclass 
class ContinuumEvent:
    """Real-time event from Continuum system"""
    type: str
    timestamp: str
    source: str
    data: Dict[str, Any]
    level: str = "info"

@dataclass
class ContinuumStatus:
    """System status information"""
    running: bool
    daemons: Dict[str, Dict[str, Any]]
    api_version: str
    uptime: int
    health: str

class EventType(Enum):
    """Event types for filtering"""
    CONSOLE = "console"
    DAEMON = "daemon"
    COMMAND = "command"
    BROWSER = "browser"
    SYSTEM = "system"

class LogLevel(Enum):
    """Log levels for filtering"""
    DEBUG = "debug"
    INFO = "info"
    WARN = "warn"
    ERROR = "error"

# Type aliases for cleaner code
CommandResult = Dict[str, Any]
EventStream = AsyncGenerator[ContinuumEvent, None]
StatusCallback = callable[[ContinuumStatus], None]