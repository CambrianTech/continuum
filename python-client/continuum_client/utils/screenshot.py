"""
Elegant Screenshot Utility
Reusable screenshot capture with selector precision and intelligent naming
"""

import asyncio
import json
import base64
from pathlib import Path
from datetime import datetime
from typing import Optional, Dict, Any
from ..core.client import ContinuumClient


class ScreenshotCapture:
    """
    Elegant screenshot capture utility with selector precision
    Supports intelligent naming and automatic file management
    """
    
    def __init__(self):
        """Initialize screenshot capture utility - continuum core handles all paths"""
        # Minimal path for client-side operations - server handles actual saving
        self.screenshots_dir = Path('.continuum/screenshots')
    
    async def capture_by_selector(
        self, 
        client: ContinuumClient,
        selector: str = 'body',
        name_prefix: str = 'screenshot',
        scale: float = 1.0,
        format: str = 'png'
    ) -> Dict[str, Any]:
        """
        Capture screenshot by CSS selector with elegant naming
        Uses proven approach from trust_the_process.py for reliability
        
        Args:
            client: Connected ContinuumClient instance
            selector: CSS selector for target element (default: 'body' for full page)
            name_prefix: Prefix for filename (default: 'screenshot')
            scale: Scale factor for capture (default: 1.0)
            format: Image format (default: 'png')
            
        Returns:
            Dict with success status, filename, path, and metadata
        """
        try:
            # Generate intelligent filename
            timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
            selector_name = self._selector_to_name(selector)
            filename = f'{name_prefix}_{selector_name}_{timestamp}.{format}'
            
            # Use proper screenshot command instead of direct html2canvas
            screenshot_js = f"""
                console.log('📸 Using continuum.command.screenshot for: {selector}');
                
                if (window.continuum && window.continuum.command && window.continuum.command.screenshot) {{
                    return window.continuum.command.screenshot({{
                        selector: '{selector}',
                        name_prefix: '{name_prefix}_{selector_name}',
                        scale: {scale},
                        manual: false
                    }});
                }} else {{
                    console.error('❌ Screenshot command not available');
                    return {{
                        success: false,
                        error: 'Screenshot command not available'
                    }};
                }}
            """
            
            result = await client.js.execute(screenshot_js)
            
            if result['success']:
                screenshot_data = json.loads(result['result'])
                if screenshot_data['success']:
                    # Save screenshot to filesystem
                    screenshot_path = self.screenshots_dir / filename
                    
                    base64_data = screenshot_data['dataURL'].split(',')[1]
                    image_bytes = base64.b64decode(base64_data)
                    
                    with open(screenshot_path, 'wb') as f:
                        f.write(image_bytes)
                    
                    return {
                        'success': True,
                        'filename': filename,
                        'path': str(screenshot_path),
                        'selector': screenshot_data.get('selector', selector),
                        'width': screenshot_data['width'],
                        'height': screenshot_data['height'],
                        'size_bytes': len(image_bytes),
                        'timestamp': timestamp
                    }
                else:
                    return {
                        'success': False,
                        'error': screenshot_data.get('error', 'Screenshot capture failed')
                    }
            else:
                return {
                    'success': False,
                    'error': f"JavaScript execution failed: {result.get('error', 'Unknown error')}"
                }
                
        except Exception as e:
            return {
                'success': False,
                'error': f"Screenshot utility error: {str(e)}"
            }
    
    def _selector_to_name(self, selector: str) -> str:
        """
        Convert CSS selector to filesystem-safe name
        
        Args:
            selector: CSS selector string
            
        Returns:
            Filesystem-safe name derived from selector
        """
        if selector == 'body':
            return 'fullpage'
        
        # Handle common selectors elegantly
        selector_mappings = {
            '.version-badge': 'version',
            '.users-agents': 'users_agents',
            '[class*="agent"]': 'agents',
            '[class*="user"]': 'users',
            '#sidebar': 'sidebar',
            '.sidebar': 'sidebar'
        }
        
        if selector in selector_mappings:
            return selector_mappings[selector]
        
        # Generic cleanup for other selectors
        clean_name = selector.replace('.', '').replace('#', '').replace('[', '').replace(']', '')
        clean_name = ''.join(c if c.isalnum() else '_' for c in clean_name)
        # Remove consecutive underscores and strip
        clean_name = '_'.join(part for part in clean_name.split('_') if part)
        clean_name = clean_name.strip('_').lower()
        
        return clean_name or 'element'


# Convenience functions for common use cases
async def capture_version_badge(client: ContinuumClient) -> Dict[str, Any]:
    """Capture version badge screenshot using proper screenshot command"""
    # Use the proper screenshot command instead of html2canvas
    result = await client.js.execute("""
        console.log('📸 Using continuum.command.screenshot for version badge');
        
        if (window.continuum && window.continuum.command && window.continuum.command.screenshot) {
            return window.continuum.command.screenshot({
                selector: '.version-badge',
                name_prefix: 'version_badge',
                scale: 2.0,
                manual: false
            });
        } else {
            console.error('❌ Screenshot command not available');
            return {
                success: false,
                error: 'Screenshot command not available'
            };
        }
    """)
    
    if result.get('success'):
        screenshot_result = result.get('result', {})
        if isinstance(screenshot_result, dict) and screenshot_result.get('success'):
            return {
                'success': True,
                'message': 'Version screenshot captured using screenshot command',
                'result': screenshot_result
            }
    
    return {
        'success': False,
        'error': 'Version screenshot failed',
        'result': result
    }


async def capture_users_agents(client: ContinuumClient) -> Dict[str, Any]:
    """Capture users & agents section with elegant naming"""
    capture = ScreenshotCapture()
    
    # Try multiple selectors for users & agents section
    selectors = [
        '[class*="agent"]',
        '[class*="user"]', 
        '#sidebar',
        '.sidebar'
    ]
    
    for selector in selectors:
        result = await capture.capture_by_selector(
            client,
            selector=selector,
            name_prefix='users_agents'
        )
        if result['success']:
            return result
    
    # Fallback to full page
    return await capture.capture_by_selector(
        client,
        selector='body',
        name_prefix='users_agents_fullpage'
    )


async def capture_full_uptime_validation(client: ContinuumClient) -> Dict[str, Any]:
    """
    Capture both version and users & agents for complete uptime validation
    Returns dict with both screenshot results
    """
    results = {}
    
    # Capture version badge
    version_result = await capture_version_badge(client)
    results['version'] = version_result
    
    # Capture users & agents
    users_result = await capture_users_agents(client)
    results['users_agents'] = users_result
    
    # Overall success if both succeeded
    results['success'] = version_result['success'] and users_result['success']
    results['message'] = 'Full uptime validation captured' if results['success'] else 'Partial capture failure'
    
    return results