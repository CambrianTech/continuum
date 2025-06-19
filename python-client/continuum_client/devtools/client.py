"""
DevTools Client
Simple HTTP-based interface for browser development tools
"""

import requests
import json
import base64
from typing import Dict, List, Optional, Union, Any
from pathlib import Path

class DevToolsClient:
    """
    Synchronous client for DevTools server
    Perfect for AI agents and personas to interact with browsers
    """
    
    def __init__(self, base_url: str = "http://localhost:9001"):
        self.base_url = base_url.rstrip('/')
        self.session = requests.Session()
        self.session.headers.update({
            'Content-Type': 'application/json',
            'User-Agent': 'DevTools-Client/1.0'
        })
    
    def is_available(self) -> bool:
        """Check if DevTools server is available"""
        try:
            response = self.session.get(f"{self.base_url}/api/status", timeout=5)
            return response.status_code == 200
        except:
            return False
    
    def get_status(self) -> Dict[str, Any]:
        """Get DevTools server and connection status"""
        response = self.session.get(f"{self.base_url}/api/status")
        response.raise_for_status()
        return response.json()
    
    def connect(self, adapter: str = "auto", **options) -> Dict[str, Any]:
        """Connect to browser DevTools"""
        data = {"adapter": adapter, **options}
        response = self.session.post(f"{self.base_url}/api/connect", json=data)
        response.raise_for_status()
        return response.json()
    
    def disconnect(self) -> Dict[str, Any]:
        """Disconnect from browser DevTools"""
        response = self.session.post(f"{self.base_url}/api/disconnect")
        response.raise_for_status()
        return response.json()
    
    def take_screenshot(self, 
                       filename: Optional[str] = None,
                       format: str = "png",
                       quality: int = 90,
                       full_page: bool = False) -> Union[Dict[str, Any], bytes]:
        """
        Take browser screenshot
        
        Args:
            filename: Save to file (optional)
            format: Image format (png, jpeg)
            quality: Image quality 1-100
            full_page: Capture full page or viewport
            
        Returns:
            Dict with result info or raw image bytes
        """
        data = {
            "format": format,
            "quality": quality,
            "fullPage": full_page,
            "filename": filename
        }
        
        response = self.session.post(f"{self.base_url}/api/screenshot", json=data)
        response.raise_for_status()
        
        if filename:
            return response.json()
        else:
            # Return raw image bytes
            return response.content
    
    def execute_script(self, script: str) -> Dict[str, Any]:
        """
        Execute JavaScript in browser
        
        Args:
            script: JavaScript code to execute
            
        Returns:
            Execution result with value and type
        """
        data = {"script": script}
        response = self.session.post(f"{self.base_url}/api/execute", json=data)
        response.raise_for_status()
        return response.json()
    
    def get_console_logs(self, limit: int = 50, level: str = "all") -> List[Dict[str, Any]]:
        """
        Get browser console logs
        
        Args:
            limit: Maximum number of logs
            level: Filter by level (log, error, warn, all)
            
        Returns:
            List of console log entries
        """
        params = {"limit": limit, "level": level}
        response = self.session.get(f"{self.base_url}/api/console", params=params)
        response.raise_for_status()
        return response.json().get("logs", [])
    
    def get_websocket_frames(self, limit: int = 100, direction: str = "all") -> List[Dict[str, Any]]:
        """
        Get WebSocket communication frames
        
        Args:
            limit: Maximum number of frames
            direction: Filter by direction (sent, received, all)
            
        Returns:
            List of WebSocket frames
        """
        params = {"limit": limit, "direction": direction}
        response = self.session.get(f"{self.base_url}/api/websocket", params=params)
        response.raise_for_status()
        return response.json().get("frames", [])
    
    def command(self, action: str, **params) -> Dict[str, Any]:
        """
        Execute generic DevTools command
        
        Args:
            action: Command action
            **params: Command parameters
            
        Returns:
            Command result
        """
        data = {"action": action, "params": params}
        response = self.session.post(f"{self.base_url}/api/command", json=data)
        response.raise_for_status()
        return response.json()
    
    # Convenience methods for common tasks
    
    def click_element(self, selector: str) -> Dict[str, Any]:
        """Click an element by CSS selector"""
        script = f"""
        const element = document.querySelector('{selector}');
        if (element) {{
            element.click();
            true;
        }} else {{
            throw new Error('Element not found: {selector}');
        }}
        """
        return self.execute_script(script)
    
    def get_element_text(self, selector: str) -> str:
        """Get text content of an element"""
        script = f"""
        const element = document.querySelector('{selector}');
        element ? element.textContent.trim() : null;
        """
        result = self.execute_script(script)
        return result.get("result", "")
    
    def set_element_value(self, selector: str, value: str) -> Dict[str, Any]:
        """Set value of an input element"""
        script = f"""
        const element = document.querySelector('{selector}');
        if (element) {{
            element.value = '{value}';
            element.dispatchEvent(new Event('input', {{bubbles: true}}));
            element.dispatchEvent(new Event('change', {{bubbles: true}}));
            true;
        }} else {{
            throw new Error('Element not found: {selector}');
        }}
        """
        return self.execute_script(script)
    
    def wait_for_element(self, selector: str, timeout: int = 5000) -> Dict[str, Any]:
        """Wait for element to appear"""
        script = f"""
        new Promise((resolve, reject) => {{
            const startTime = Date.now();
            const check = () => {{
                const element = document.querySelector('{selector}');
                if (element) {{
                    resolve(true);
                }} else if (Date.now() - startTime > {timeout}) {{
                    reject(new Error('Timeout waiting for element: {selector}'));
                }} else {{
                    setTimeout(check, 100);
                }}
            }};
            check();
        }});
        """
        return self.execute_script(script)
    
    def scroll_to_element(self, selector: str) -> Dict[str, Any]:
        """Scroll element into view"""
        script = f"""
        const element = document.querySelector('{selector}');
        if (element) {{
            element.scrollIntoView({{behavior: 'smooth', block: 'center'}});
            true;
        }} else {{
            throw new Error('Element not found: {selector}');
        }}
        """
        return self.execute_script(script)
    
    def get_page_info(self) -> Dict[str, Any]:
        """Get basic page information"""
        script = """
        ({
            title: document.title,
            url: window.location.href,
            domain: window.location.hostname,
            readyState: document.readyState,
            viewport: {
                width: window.innerWidth,
                height: window.innerHeight
            },
            scroll: {
                x: window.scrollX,
                y: window.scrollY
            }
        })
        """
        result = self.execute_script(script)
        return result.get("result", {})
    
    def find_elements(self, selector: str) -> List[Dict[str, Any]]:
        """Find all elements matching selector"""
        script = f"""
        Array.from(document.querySelectorAll('{selector}')).map(el => {{
            const rect = el.getBoundingClientRect();
            return {{
                tagName: el.tagName,
                id: el.id,
                className: el.className,
                textContent: el.textContent.trim().substring(0, 100),
                href: el.href || null,
                src: el.src || null,
                value: el.value || null,
                rect: {{
                    x: rect.x,
                    y: rect.y,
                    width: rect.width,
                    height: rect.height
                }},
                visible: rect.width > 0 && rect.height > 0
            }};
        }});
        """
        result = self.execute_script(script)
        return result.get("result", [])
    
    def save_screenshot(self, filepath: str, **kwargs) -> str:
        """Save screenshot to file and return path"""
        image_data = self.take_screenshot(**kwargs)
        
        if isinstance(image_data, bytes):
            # Raw image data
            path = Path(filepath)
            path.write_bytes(image_data)
            return str(path.absolute())
        else:
            # Response included filepath
            return image_data.get("filepath", filepath)