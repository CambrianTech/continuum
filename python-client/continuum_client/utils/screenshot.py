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
    
    def __init__(self, screenshots_dir: Optional[Path] = None):
        """
        Initialize screenshot capture utility
        
        Args:
            screenshots_dir: Directory to save screenshots (default: .continuum/screenshots)
        """
        if screenshots_dir is None:
            # Auto-detect .continuum/screenshots directory
            current_dir = Path.cwd()
            while current_dir.parent != current_dir:
                continuum_dir = current_dir / '.continuum' / 'screenshots'
                if continuum_dir.parent.exists():
                    screenshots_dir = continuum_dir
                    break
                current_dir = current_dir.parent
            
            if screenshots_dir is None:
                screenshots_dir = Path('.continuum/screenshots')
        
        self.screenshots_dir = Path(screenshots_dir)
        self.screenshots_dir.mkdir(parents=True, exist_ok=True)
    
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
            
            # Use proven working approach from trust_the_process.py
            if selector == 'body':
                # Full page capture - use working selectors for agents section
                screenshot_js = """
                console.log('📸 Elegant full page capture');
                
                const selectors = [
                    '[id*="agent"]',
                    '[class*="agent"]', 
                    '#sidebar',
                    '.sidebar',
                    '[class*="user"]'
                ];
                
                let targetElement = null;
                let targetSelector = null;
                
                for (const selector of selectors) {
                    const element = document.querySelector(selector);
                    if (element && element.offsetWidth > 0 && element.offsetHeight > 0) {
                        targetElement = element;
                        targetSelector = selector;
                        break;
                    }
                }
                
                if (!targetElement) {
                    targetElement = document.body;
                    targetSelector = 'body';
                }
                
                console.log('📸 Capturing', targetSelector);
                
                return new Promise((resolve) => {
                    html2canvas(targetElement, {
                        allowTaint: true,
                        useCORS: true,
                        scale: 1,
                        backgroundColor: '#1a1a1a'
                    }).then(function(canvas) {
                        const dataURL = canvas.toDataURL('image/png');
                        resolve({
                            success: true,
                            dataURL: dataURL,
                            width: canvas.width,
                            height: canvas.height,
                            selector: targetSelector
                        });
                    }).catch(function(error) {
                        console.error('📸 Screenshot failed:', error);
                        resolve({
                            success: false,
                            error: error.message
                        });
                    });
                });
                """
            else:
                # Specific selector capture
                screenshot_js = f"""
                console.log('📸 Elegant selector capture: {selector}');
                
                const targetElement = document.querySelector('{selector}');
                if (!targetElement) {{
                    return {{success: false, error: 'Selector not found: {selector}'}};
                }}
                
                return new Promise((resolve) => {{
                    html2canvas(targetElement, {{
                        allowTaint: true,
                        useCORS: true,
                        scale: {scale},
                        backgroundColor: null
                    }}).then(function(canvas) {{
                        const dataURL = canvas.toDataURL('image/{format}');
                        resolve({{
                            success: true,
                            dataURL: dataURL,
                            width: canvas.width,
                            height: canvas.height,
                            selector: '{selector}'
                        }});
                    }}).catch(function(error) {{
                        console.error('📸 Screenshot failed:', error);
                        resolve({{
                            success: false,
                            error: error.message
                        }});
                    }});
                }});
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
    """Capture version badge screenshot with elegant naming"""
    capture = ScreenshotCapture()
    return await capture.capture_by_selector(
        client, 
        selector='.version-badge', 
        name_prefix='version',
        scale=2.0
    )


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