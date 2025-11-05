"""
JavaScript Execution Exceptions
Clean error handling for promise-like JavaScript execution
"""

class ContinuumError(Exception):
    """Base exception for all Continuum client errors"""
    pass

class ConnectionError(ContinuumError):
    """WebSocket connection failed or lost"""
    pass

class JSExecutionError(ContinuumError):
    """JavaScript code execution failed (promise rejected)"""
    def __init__(self, message, js_error=None, stack_trace=None):
        super().__init__(message)
        self.js_error = js_error
        self.stack_trace = stack_trace
        
    def __str__(self):
        base = super().__str__()
        if self.js_error:
            base += f" | JS Error: {self.js_error}"
        return base

class JSTimeoutError(JSExecutionError):
    """JavaScript execution timed out"""
    pass

class JSSyntaxError(JSExecutionError):
    """JavaScript syntax error"""
    pass