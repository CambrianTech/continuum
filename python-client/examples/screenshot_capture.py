#!/usr/bin/env python3
"""
Screenshot Capture Example

This example demonstrates how to capture screenshots of web page elements
using the Continuum Promise Post Office System.

Features:
- Capture full page or specific elements by selector
- Save to files or display directly
- Multiple image formats (PNG, JPEG, WebP)
- Automatic html2canvas loading
- Element detection and fallback options
"""

import asyncio
import json
import base64
import subprocess
import tempfile
import os
import sys
from pathlib import Path

# Add parent directory to path so we can import continuum_client
sys.path.insert(0, str(Path(__file__).parent.parent))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class ScreenshotCapture:
    """Easy-to-use screenshot capture for Continuum web pages"""
    
    def __init__(self):
        load_continuum_config()
        self.client = None
        
    async def __aenter__(self):
        self.client = ContinuumClient()
        await self.client.__aenter__()
        await self.client.register_agent({
            'agentId': 'screenshot-capture-example',
            'agentName': 'Screenshot Capture Example',
            'agentType': 'ai'
        })
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.client:
            await self.client.__aexit__(exc_type, exc_val, exc_tb)
    
    async def find_element(self, search_terms):
        """Find an element by searching for various selectors and text content"""
        
        if isinstance(search_terms, str):
            search_terms = [search_terms]
        
        find_js = f"""
        return new Promise((resolve) => {{
            const searchTerms = {json.dumps(search_terms)};
            let foundElement = null;
            let foundSelector = null;
            
            // Try direct selectors first
            for (const term of searchTerms) {{
                // Try as direct selector
                try {{
                    const element = document.querySelector(term);
                    if (element) {{
                        foundElement = element;
                        foundSelector = term;
                        break;
                    }}
                }} catch(e) {{
                    // Not a valid selector, continue
                }}
                
                // Try as ID
                const byId = document.getElementById(term);
                if (byId) {{
                    foundElement = byId;
                    foundSelector = '#' + term;
                    break;
                }}
                
                // Try as class
                const byClass = document.querySelector('.' + term);
                if (byClass) {{
                    foundElement = byClass;
                    foundSelector = '.' + term;
                    break;
                }}
                
                // Try selectors containing the term
                const containsId = document.querySelector('[id*="' + term + '"]');
                if (containsId) {{
                    foundElement = containsId;
                    foundSelector = '[id*="' + term + '"]';
                    break;
                }}
                
                const containsClass = document.querySelector('[class*="' + term + '"]');
                if (containsClass) {{
                    foundElement = containsClass;
                    foundSelector = '[class*="' + term + '"]';
                    break;
                }}
            }}
            
            // Fallback: search by text content
            if (!foundElement) {{
                const allElements = document.querySelectorAll('*');
                for (const term of searchTerms) {{
                    for (const el of allElements) {{
                        if (el.textContent && el.textContent.toLowerCase().includes(term.toLowerCase())) {{
                            const rect = el.getBoundingClientRect();
                            if (rect.width > 50 && rect.height > 20) {{
                                foundElement = el;
                                foundSelector = el.tagName.toLowerCase() + 
                                              (el.id ? '#' + el.id : '') + 
                                              (el.className ? '.' + el.className.split(' ')[0] : '');
                                break;
                            }}
                        }}
                    }}
                    if (foundElement) break;
                }}
            }}
            
            if (foundElement) {{
                const rect = foundElement.getBoundingClientRect();
                resolve(JSON.stringify({{
                    found: true,
                    selector: foundSelector,
                    width: rect.width,
                    height: rect.height,
                    visible: rect.width > 0 && rect.height > 0,
                    tag: foundElement.tagName.toLowerCase(),
                    id: foundElement.id || null,
                    className: foundElement.className || null
                }}));
            }} else {{
                resolve(JSON.stringify({{
                    found: false,
                    searchTerms: searchTerms
                }}));
            }}
        }});
        """
        
        result_json = await self.client.js.get_value(find_js, timeout=10)
        return json.loads(result_json)
    
    async def capture(self, selector='body', format='png', quality=0.9, save_path=None, open_image=False):
        """
        Capture a screenshot of a web page element
        
        Args:
            selector (str): CSS selector, element ID, class, or search term
            format (str): Image format - 'png', 'jpeg', 'webp'
            quality (float): Image quality for JPEG/WebP (0.0-1.0)
            save_path (str): Path to save image (optional)
            open_image (bool): Whether to open the image after capture
            
        Returns:
            dict: Screenshot result with success, dataURL, dimensions, etc.
        """
        
        # If selector doesn't look like CSS, try to find the element
        if not any(char in selector for char in '.#[]>+~:'):
            print(f"🔍 Searching for element containing: {selector}")
            find_result = await self.find_element(selector)
            if find_result['found']:
                selector = find_result['selector']
                print(f"✅ Found element: {selector} ({find_result['width']}x{find_result['height']})")
            else:
                print(f"❌ Element not found, using body instead")
                selector = 'body'
        
        # Ensure format is a string and lowercase
        format = str(format or 'png').lower()
        
        mime_type = {
            'png': 'image/png',
            'jpeg': 'image/jpeg', 
            'jpg': 'image/jpeg',
            'webp': 'image/webp'
        }.get(format, 'image/png')
        
        screenshot_js = f"""
        return new Promise((resolve, reject) => {{
            console.log('📸 Starting screenshot of: {selector}');
            
            // Load html2canvas if not available
            if (typeof html2canvas === 'undefined') {{
                const script = document.createElement('script');
                script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                script.onload = () => captureElement();
                script.onerror = () => reject('Failed to load html2canvas');
                document.head.appendChild(script);
            }} else {{
                captureElement();
            }}
            
            function captureElement() {{
                const targetElement = document.querySelector('{selector}');
                if (!targetElement) {{
                    reject('Element not found: {selector}');
                    return;
                }}
                
                // Scroll element into view
                targetElement.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
                
                // Wait for scroll to complete
                setTimeout(() => {{
                    html2canvas(targetElement, {{
                        allowTaint: true,
                        useCORS: true,
                        scale: 1,
                        backgroundColor: null,
                        logging: false
                    }}).then(canvas => {{
                        let dataURL;
                        if ('{format}' === 'jpeg' || '{format}' === 'jpg') {{
                            dataURL = canvas.toDataURL('{mime_type}', {quality});
                        }} else if ('{format}' === 'webp') {{
                            dataURL = canvas.toDataURL('{mime_type}', {quality});
                        }} else {{
                            dataURL = canvas.toDataURL('{mime_type}');
                        }}
                        
                        resolve(JSON.stringify({{
                            success: true,
                            dataURL: dataURL,
                            width: canvas.width,
                            height: canvas.height,
                            selector: '{selector}',
                            format: '{format}',
                            mimeType: '{mime_type}'
                        }}));
                    }}).catch(error => {{
                        reject(error.message);
                    }});
                }}, 300);
            }}
        }});
        """
        
        print(f"📸 Capturing screenshot...")
        result_json = await self.client.js.get_value(screenshot_js, timeout=30)
        result = json.loads(result_json)
        
        if not result['success']:
            raise Exception("Screenshot capture failed")
        
        print(f"✅ Screenshot captured: {result['width']}x{result['height']}")
        
        # Save to file if requested
        if save_path or open_image:
            base64_data = result['dataURL'].split(',')[1]
            image_bytes = base64.b64decode(base64_data)
            
            if save_path:
                # Ensure directory exists
                save_path = Path(save_path)
                save_path.parent.mkdir(parents=True, exist_ok=True)
                
                with open(save_path, 'wb') as f:
                    f.write(image_bytes)
                print(f"💾 Saved to: {save_path}")
                result['saved_path'] = str(save_path)
                
                if open_image:
                    self._open_image(save_path)
            
            elif open_image:
                # Create temporary file
                with tempfile.NamedTemporaryFile(delete=False, suffix=f'.{format}') as temp_file:
                    temp_file.write(image_bytes)
                    temp_path = temp_file.name
                
                print(f"💾 Saved to temporary file: {temp_path}")
                result['temp_path'] = temp_path
                self._open_image(temp_path)
        
        return result
    
    def _open_image(self, image_path):
        """Open image with default system viewer"""
        try:
            if os.name == 'nt':  # Windows
                subprocess.run(['start', str(image_path)], shell=True)
            elif 'darwin' in os.uname().sysname.lower():  # macOS
                subprocess.run(['open', str(image_path)])
            else:  # Linux
                subprocess.run(['xdg-open', str(image_path)])
            print("🖼️ Image opened in default viewer")
        except Exception as e:
            print(f"❌ Error opening image: {e}")

# Example usage functions
async def capture_full_page():
    """Capture full page screenshot"""
    async with ScreenshotCapture() as capture:
        result = await capture.capture(
            selector='body',
            format='png',
            save_path='screenshots/full_page.png',
            open_image=True
        )
        return result

async def capture_element_by_id(element_id):
    """Capture specific element by ID"""
    async with ScreenshotCapture() as capture:
        result = await capture.capture(
            selector=f'#{element_id}',
            format='png',
            open_image=True
        )
        return result

async def capture_by_search(search_term):
    """Capture element by searching for text content"""
    async with ScreenshotCapture() as capture:
        result = await capture.capture(
            selector=search_term,  # Will auto-search for this term
            format='png',
            open_image=True
        )
        return result

async def capture_multiple_formats(selector):
    """Capture same element in multiple formats"""
    async with ScreenshotCapture() as capture:
        formats = [
            ('png', 'screenshots/element.png'),
            ('jpeg', 'screenshots/element.jpg'),
            ('webp', 'screenshots/element.webp')
        ]
        
        results = []
        for format_name, save_path in formats:
            result = await capture.capture(
                selector=selector,
                format=format_name,
                quality=0.8,
                save_path=save_path
            )
            results.append(result)
        
        return results

# Command line interface
async def main():
    """Interactive screenshot capture"""
    print("📸 Continuum Screenshot Capture Example")
    print("=" * 40)
    
    # Example captures
    examples = [
        ("Full page", capture_full_page),
        ("Search for 'agents'", lambda: capture_by_search('agents')),
        ("Search for 'user'", lambda: capture_by_search('user')), 
        ("Search for 'sidebar'", lambda: capture_by_search('sidebar')),
    ]
    
    for name, func in examples:
        try:
            print(f"\n🎯 Example: {name}")
            result = await func()
            print(f"   ✅ Success: {result.get('width', 0)}x{result.get('height', 0)} pixels")
        except Exception as e:
            print(f"   ❌ Failed: {e}")
        
        # Wait between captures
        await asyncio.sleep(2)

if __name__ == "__main__":
    asyncio.run(main())