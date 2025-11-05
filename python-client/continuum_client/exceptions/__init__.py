"""
Continuum Client Exceptions
Custom exception classes for JavaScript execution and connection errors
"""

from .js_errors import JSExecutionError, JSTimeoutError, JSSyntaxError, ConnectionError

__all__ = [
    "JSExecutionError",
    "JSTimeoutError", 
    "JSSyntaxError",
    "ConnectionError"
]