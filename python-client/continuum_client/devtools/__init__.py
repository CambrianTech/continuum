"""
DevTools Client Package
Universal interface for AI agents to use browser development tools
"""

from .client import DevToolsClient
from .log_monitor import DevToolsLogMonitor
from .browser_adapters import create_adapter, auto_detect_browsers
from .live_monitor import LiveDevToolsMonitor

__all__ = ['DevToolsClient', 'DevToolsLogMonitor', 'LiveDevToolsMonitor', 'create_adapter', 'auto_detect_browsers']