#!/usr/bin/env python3
"""
Capture screenshot of the user & agents section and display it
"""

import asyncio
import json
import base64
import subprocess
import tempfile
from pathlib import Path
from datetime import datetime
import os
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def capture_agents_section():
    """Capture the user & agents section by element ID and show it"""
    
    load_continuum_config()
    
    print("📸 Capturing user & agents section...")
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'screenshot-agents-section',
            'agentName': 'Screenshot Agents Section',
            'agentType': 'ai'
        })
        
        # First, let's find the agents section element
        find_element_js = """
        return new Promise((resolve) => {
            // Look for common agent section IDs/classes
            const possibleSelectors = [
                '#agents',
                '#user-agents',
                '#agent-list',
                '#agents-section',
                '.agents',
                '.agent-list',
                '[id*="agent"]',
                '[class*="agent"]'
            ];
            
            let foundElement = null;
            let foundSelector = null;
            
            for (const selector of possibleSelectors) {
                const element = document.querySelector(selector);
                if (element) {
                    foundElement = element;
                    foundSelector = selector;
                    break;
                }
            }
            
            if (foundElement) {
                const rect = foundElement.getBoundingClientRect();
                resolve(JSON.stringify({
                    found: true,
                    selector: foundSelector,
                    width: rect.width,
                    height: rect.height,
                    visible: rect.width > 0 && rect.height > 0,
                    innerHTML: foundElement.innerHTML.substring(0, 200) + '...'
                }));
            } else {
                // Fallback: look for any element containing "agent" text
                const allElements = document.querySelectorAll('*');
                for (const el of allElements) {
                    if (el.textContent && el.textContent.toLowerCase().includes('agent')) {
                        const rect = el.getBoundingClientRect();
                        if (rect.width > 50 && rect.height > 20) {
                            resolve(JSON.stringify({
                                found: true,
                                selector: el.tagName + (el.id ? '#' + el.id : '') + (el.className ? '.' + el.className.split(' ')[0] : ''),
                                width: rect.width,
                                height: rect.height,
                                visible: true,
                                innerHTML: el.innerHTML.substring(0, 200) + '...',
                                fallback: true
                            }));
                            return;
                        }
                    }
                }
                
                resolve(JSON.stringify({
                    found: false,
                    message: 'No agents section found'
                }));
            }
        });
        """
        
        try:
            result_json = await client.js.get_value(find_element_js, timeout=10)
            result = json.loads(result_json)
            
            if not result['found']:
                print("❌ No agents section found in the page")
                print("Let me try to capture the whole page instead...")
                selector = 'body'
            else:
                selector = result['selector']
                print(f"✅ Found agents section: {selector}")
                print(f"   Dimensions: {result['width']}x{result['height']}")
                if result.get('fallback'):
                    print("   (Found via text search fallback)")
                
        except Exception as e:
            print(f"⚠️ Error finding element: {e}")
            print("Falling back to full page screenshot...")
            selector = 'body'
        
        # Now capture the screenshot of the found element
        screenshot_js = f"""
        return new Promise((resolve, reject) => {{
            console.log('📸 Starting screenshot of: {selector}');
            
            // Load html2canvas if not available
            if (typeof html2canvas === 'undefined') {{
                const script = document.createElement('script');
                script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                script.onload = () => {{
                    console.log('📸 html2canvas loaded, capturing...');
                    captureElement();
                }};
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
                
                // Wait a moment for scroll to complete
                setTimeout(() => {{
                    html2canvas(targetElement, {{
                        allowTaint: true,
                        useCORS: true,
                        scale: 1,
                        backgroundColor: '#1a1a1a',
                        logging: true
                    }}).then(canvas => {{
                        const dataURL = canvas.toDataURL('image/png');
                        console.log('✅ Screenshot captured successfully');
                        resolve(JSON.stringify({{
                            success: true,
                            dataURL: dataURL,
                            width: canvas.width,
                            height: canvas.height,
                            selector: '{selector}',
                            format: 'png'
                        }}));
                    }}).catch(error => {{
                        console.error('📸 Screenshot failed:', error);
                        reject(error.message);
                    }});
                }}, 500);
            }}
        }});
        """
        
        try:
            print(f"📸 Capturing screenshot of: {selector}")
            result_json = await client.js.get_value(screenshot_js, timeout=30)
            result = json.loads(result_json)
            
            if not result['success']:
                print("❌ Screenshot capture failed")
                return
            
            print(f"✅ Screenshot captured!")
            print(f"   Size: {result['width']}x{result['height']}")
            print(f"   Element: {result['selector']}")
            print(f"   Data size: {len(result['dataURL'])} characters")
            
            # Extract base64 data and save to .continuum/screenshots/
            base64_data = result['dataURL'].split(',')[1]
            image_bytes = base64.b64decode(base64_data)
            
            # Create .continuum/screenshots directory if it doesn't exist
            screenshots_dir = Path('.continuum/screenshots')
            screenshots_dir.mkdir(parents=True, exist_ok=True)
            
            # Create filename with timestamp
            timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
            filename = f'agents_section_{timestamp}.png'
            screenshot_path = screenshots_dir / filename
            
            # Save screenshot
            with open(screenshot_path, 'wb') as f:
                f.write(image_bytes)
            
            temp_path = str(screenshot_path)
            
            print(f"💾 Saved screenshot to: {temp_path}")
            print(f"📁 Opening image...")
            
            # Open the image using the default system viewer
            try:
                if os.name == 'nt':  # Windows
                    subprocess.run(['start', temp_path], shell=True)
                elif os.name == 'posix':  # macOS/Linux
                    if 'darwin' in os.uname().sysname.lower():  # macOS
                        subprocess.run(['open', temp_path])
                    else:  # Linux
                        subprocess.run(['xdg-open', temp_path])
                
                print("🖼️ Image should now be open in your default image viewer!")
                print(f"📍 File location: {temp_path}")
                
                # Keep the file permanently in .continuum/screenshots/
                print(f"\n✅ Screenshot saved permanently to: {temp_path}")
                print("📁 File will remain in .continuum/screenshots/ for future reference")
                    
            except Exception as e:
                print(f"❌ Error opening image: {e}")
                print(f"📁 File saved at: {temp_path}")
                
        except Exception as e:
            print(f"❌ Screenshot capture failed: {e}")

if __name__ == "__main__":
    asyncio.run(capture_agents_section())