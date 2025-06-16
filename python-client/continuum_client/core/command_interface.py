"""
Universal Command Interface
Elegant, consistent API across all clients: continuum.command.screenshot()
"""

from typing import Dict, Any, Optional

class CommandInterface:
    """
    Universal command interface providing elegant APIs like:
    - await continuum.command.screenshot()
    - await continuum.command.screenshot(selector='.version-badge')
    """
    
    def __init__(self, client):
        self.client = client
    
    async def screenshot(self, selector: str = 'body', name_prefix: str = 'screenshot', scale: float = 1.0, manual: bool = False, subdirectory: str = None) -> Dict[str, Any]:
        """
        Universal screenshot command - dogmatically simple
        
        Args:
            selector: CSS selector for target element (default: 'body')
            name_prefix: Prefix for filename (default: 'screenshot') 
            scale: Scale factor for capture (default: 1.0)
            manual: Manual mode for user-guided screenshots (default: False)
            subdirectory: Subdirectory path for organized storage (default: None)
            
        Returns:
            Dict with success status, filename, and response details
        """
        params = {
            'selector': selector,
            'name_prefix': name_prefix,
            'scale': scale,
            'manual': manual,
            'source': 'python_client_universal'
        }
        
        # Add subdirectory if specified
        if subdirectory:
            params['subdirectory'] = subdirectory
        
        try:
            result = await self.client.send_command('SCREENSHOT', params)
            
            return {
                'success': result.get('success', False),
                'message': result.get('message', 'Screenshot command completed'),
                'filename': result.get('data', {}).get('filename'),
                'path': result.get('data', {}).get('path'),
                'server_response': result
            }
            
        except Exception as e:
            return {
                'success': False,
                'error': f'Screenshot command failed: {str(e)}',
                'message': 'Universal screenshot API error'
            }


class ScreenshotInterface:
    """
    Chainable screenshot interface for advanced usage:
    continuum.command.screenshot().manual()
    """
    
    def __init__(self, command_interface):
        self.command_interface = command_interface
        self._params = {}
    
    def selector(self, css_selector: str):
        """Set CSS selector"""
        self._params['selector'] = css_selector
        return self
    
    def prefix(self, name_prefix: str):
        """Set filename prefix"""
        self._params['name_prefix'] = name_prefix
        return self
        
    def scale(self, scale_factor: float):
        """Set scale factor"""
        self._params['scale'] = scale_factor
        return self
    
    def manual(self):
        """Enable manual mode"""
        self._params['manual'] = True
        return self
    
    def subdirectory(self, subdir: str):
        """Set subdirectory for organized storage"""
        self._params['subdirectory'] = subdir
        return self
    
    async def execute(self) -> Dict[str, Any]:
        """Execute the screenshot with configured parameters"""
        return await self.command_interface.screenshot(**self._params)
    
    def __await__(self):
        """Make the interface awaitable: await continuum.command.screenshot()"""
        return self.execute().__await__()