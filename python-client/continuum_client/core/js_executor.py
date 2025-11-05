"""
JavaScript Executor
Promise-based JavaScript execution with clean async interface
"""

import asyncio
import json
import base64
from typing import Any, Dict, List, Optional, Union
from ..exceptions.js_errors import JSExecutionError, JSTimeoutError, JSSyntaxError

class JSExecutor:
    """
    Promise-based JavaScript executor for browser interaction
    Provides clean async interface that mimics JavaScript promises
    """
    
    def __init__(self, websocket, timeout_default: float = 10.0):
        self.ws = websocket
        self.timeout_default = timeout_default
        self.pending_executions = {}
        
    async def execute(self, js_code: str, *, 
                     timeout: Optional[float] = None,
                     expect_return: bool = False,
                     encoding: str = 'base64') -> Dict[str, Any]:
        """
        Execute JavaScript code and return promise-like result
        
        Args:
            js_code: JavaScript code to execute
            timeout: Execution timeout in seconds
            expect_return: Whether code should return a value
            encoding: Encoding for transmission ('base64' or 'utf8')
            
        Returns:
            Dict with success, result, output, error fields
            
        Raises:
            JSExecutionError: If JavaScript execution fails
            JSTimeoutError: If execution times out
            JSSyntaxError: If JavaScript has syntax errors
        """
        timeout = timeout or self.timeout_default
        
        # Prepare JavaScript code
        final_code = js_code
        if expect_return and not js_code.strip().startswith('return'):
            # Only wrap in return if it's a simple expression, not a statement
            if (';' not in js_code and 
                not any(keyword in js_code for keyword in ['console.', 'document.', 'window.', 'if', 'for', 'function', 'var', 'let', 'const'])):
                final_code = f"return ({js_code});"
            else:
                # For complex code, wrap in an immediately invoked function
                final_code = f"(function() {{ {js_code}; }})();"
            
        # Encode for transmission
        if encoding == 'base64':
            encoded_code = base64.b64encode(final_code.encode()).decode()
            task = {
                'type': 'task',
                'role': 'system', 
                'task': f'[CMD:BROWSER_JS] {encoded_code}'
            }
        else:
            task = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {final_code}'
            }
            
        # Create promise-like future
        future = asyncio.Future()
        
        # Use a simple approach: just wait for any js_executed response
        # The server will handle execution ID matching internally
        self.pending_executions['_latest'] = future
        
        try:
            # Send request
            await self.ws.send(json.dumps(task))
            
            # Wait for response (promise-like behavior)
            result = await asyncio.wait_for(future, timeout=timeout)
            
            # Handle success/error like promise resolve/reject
            if result.get('success'):
                return {
                    'success': True,
                    'result': result.get('result'),
                    'output': result.get('output', []),
                    'error': None
                }
            else:
                error_msg = result.get('error', 'Unknown JavaScript error')
                if 'timeout' in error_msg.lower():
                    raise JSTimeoutError(error_msg)
                elif 'syntax' in error_msg.lower():
                    raise JSSyntaxError(error_msg, js_error=result.get('error'))
                else:
                    raise JSExecutionError(error_msg, js_error=result.get('error'))
                    
        except asyncio.TimeoutError:
            raise JSTimeoutError(f"JavaScript execution timed out after {timeout}s")
        finally:
            # Cleanup
            self.pending_executions.pop('_latest', None)
    
    async def get_value(self, js_expression: str, **kwargs) -> Any:
        """
        Execute JavaScript and return the value (like promise.then())
        
        Args:
            js_expression: JavaScript expression that returns a value
            **kwargs: Additional arguments for execute()
            
        Returns:
            The JavaScript return value
            
        Raises:
            JSExecutionError: If execution fails
        """
        result = await self.execute(js_expression, expect_return=True, **kwargs)
        return result['result']
    
    async def run(self, js_code: str, **kwargs) -> List[Dict]:
        """
        Execute JavaScript for side effects and return console output
        
        Args:
            js_code: JavaScript code to execute
            **kwargs: Additional arguments for execute()
            
        Returns:
            List of console output entries
            
        Raises:
            JSExecutionError: If execution fails
        """
        result = await self.execute(js_code, **kwargs)
        return result['output']
    
    async def query_dom(self, selector: str, property: Optional[str] = None) -> Any:
        """
        Query DOM elements and extract data
        
        Args:
            selector: CSS selector
            property: Optional property to extract from elements
            
        Returns:
            DOM query results
        """
        if property:
            js_code = f'''
            const elements = Array.from(document.querySelectorAll("{selector}"));
            return elements.map(el => el.{property});
            '''
        else:
            js_code = f'''
            const elements = Array.from(document.querySelectorAll("{selector}"));
            return elements.map(el => {{
                tagName: el.tagName,
                textContent: el.textContent?.trim(),
                innerHTML: el.innerHTML,
                className: el.className,
                id: el.id
            }});
            '''
        
        return await self.get_value(js_code)
    
    async def wait_for_element(self, selector: str, timeout: float = 5.0) -> bool:
        """
        Wait for element to appear in DOM
        
        Args:
            selector: CSS selector to wait for
            timeout: Maximum wait time
            
        Returns:
            True if element appeared
            
        Raises:
            JSTimeoutError: If element doesn't appear within timeout
        """
        js_code = f'''
        return new Promise((resolve, reject) => {{
            const checkElement = () => {{
                const element = document.querySelector("{selector}");
                if (element) {{
                    resolve(true);
                }} else {{
                    setTimeout(checkElement, 100);
                }}
            }};
            
            setTimeout(() => reject(new Error("Element not found within timeout")), {timeout * 1000});
            checkElement();
        }});
        '''
        
        return await self.get_value(js_code, timeout=timeout + 1)
    
    async def batch(self, operations: List[Dict]) -> List[Dict]:
        """
        Execute multiple JavaScript operations in sequence
        
        Args:
            operations: List of {code, expect_return} dicts
            
        Returns:
            List of execution results
        """
        results = []
        for op in operations:
            try:
                result = await self.execute(
                    op['code'], 
                    expect_return=op.get('expect_return', False)
                )
                results.append(result)
            except JSExecutionError as e:
                results.append({'success': False, 'error': str(e)})
                break  # Stop on first failure
        return results
    
    def handle_ws_message(self, message: Dict):
        """
        Handle incoming WebSocket message (called by ContinuumClient)
        Routes js_executed responses to appropriate pending executions
        """
        if message.get('type') == 'js_executed':
            # Extract the actual data from the broadcast message
            data = message.get('data', message)
            execution_id = data.get('executionId')
            
            # Try the latest pending execution first (simplest approach)
            if '_latest' in self.pending_executions:
                future = self.pending_executions['_latest']
                if not future.done():
                    future.set_result(data)
                    self.pending_executions.pop('_latest', None)
                    return
            
            # Try to match by server's execution ID
            if execution_id and execution_id in self.pending_executions:
                future = self.pending_executions[execution_id]
                if not future.done():
                    future.set_result(data)
                    self.pending_executions.pop(execution_id, None)
                    return
            
            # Final fallback: resolve any pending execution
            if self.pending_executions:
                key, future = next(iter(self.pending_executions.items()))
                if not future.done():
                    future.set_result(data)
                    self.pending_executions.pop(key, None)