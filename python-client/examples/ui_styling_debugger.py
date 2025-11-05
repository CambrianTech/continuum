#!/usr/bin/env python3
"""
UI Styling Debugger - Reusable tool for before/after styling fixes
🎨 Visual debugging utility for CSS styling issues
"""
import asyncio
import json
import sys
import base64
from datetime import datetime
from pathlib import Path

sys.path.append('/Users/joel/Development/ideem/vHSM/externals/continuum/python-client')

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class UIStylingDebugger:
    """
    Reusable utility for visual debugging of UI styling issues.
    Provides before/after screenshot capture with CSS fix application.
    """
    
    def __init__(self, component_name="component"):
        self.component_name = component_name
        self.screenshots_dir = Path("/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/screenshots")
        self.screenshots_dir.mkdir(parents=True, exist_ok=True)
        self.timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        
    async def capture_before_after_styling_fix(self, client, selector, css_fixes, description="styling"):
        """
        Main method to capture before/after screenshots with CSS fixes applied.
        
        Args:
            client: ContinuumClient instance
            selector: CSS selector for the component to capture
            css_fixes: CSS string to apply as fixes
            description: Description of what's being fixed
        
        Returns:
            dict: Paths to before/after screenshots
        """
        print(f"🎨 UI STYLING DEBUGGER: {description}")
        print("=" * 50)
        
        # Step 1: Capture BEFORE screenshot
        print("📸 Taking BEFORE screenshot...")
        before_path = await self._capture_screenshot(
            client, selector, f"{self.component_name}_BEFORE_{self.timestamp}"
        )
        
        if not before_path:
            print("❌ BEFORE screenshot failed")
            return None
            
        print(f"✅ BEFORE screenshot saved: {before_path}")
        
        # Step 2: Apply CSS fixes
        print(f"\n🎨 Applying CSS fixes...")
        fix_success = await self._apply_css_fixes(client, selector, css_fixes)
        
        if not fix_success:
            print("❌ CSS fixes failed to apply")
            return None
            
        print("✅ CSS fixes applied successfully!")
        
        # Wait for styles to apply
        await asyncio.sleep(1)
        
        # Step 3: Capture AFTER screenshot
        print("\n📸 Taking AFTER screenshot...")
        after_path = await self._capture_screenshot(
            client, selector, f"{self.component_name}_AFTER_{self.timestamp}"
        )
        
        if not after_path:
            print("❌ AFTER screenshot failed")
            return None
            
        print(f"✅ AFTER screenshot saved: {after_path}")
        
        # Success summary
        print(f"\n🎉 SUCCESS! {description} fixes documented:")
        print(f"   BEFORE: {before_path}")
        print(f"   AFTER:  {after_path}")
        
        return {
            'before': before_path,
            'after': after_path,
            'component': self.component_name,
            'timestamp': self.timestamp
        }
    
    async def _capture_screenshot(self, client, selector, filename):
        """Capture screenshot of specified element"""
        screenshot_js = f"""
        return new Promise((resolve) => {{
            const element = document.querySelector('{selector}');
            if (!element) {{
                resolve(JSON.stringify({{
                    success: false,
                    error: 'Element not found: {selector}'
                }}));
                return;
            }}
            
            html2canvas(element, {{
                scale: 0.8,
                useCORS: true,
                backgroundColor: null
            }}).then(canvas => {{
                resolve(JSON.stringify({{
                    success: true,
                    dataUrl: canvas.toDataURL('image/png', 0.9)
                }}));
            }}).catch(error => {{
                resolve(JSON.stringify({{
                    success: false,
                    error: 'Screenshot failed: ' + error.message
                }}));
            }});
        }});
        """
        
        try:
            result = await client.js.get_value(screenshot_js, timeout=15)
            data = json.loads(result)
            
            if data.get('success'):
                # Save screenshot
                base64_data = data['dataUrl'].split(',')[1]
                image_data = base64.b64decode(base64_data)
                
                filepath = self.screenshots_dir / f"{filename}.png"
                with open(filepath, 'wb') as f:
                    f.write(image_data)
                
                return str(filepath)
            else:
                print(f"Screenshot error: {data.get('error')}")
                return None
                
        except Exception as e:
            print(f"Screenshot exception: {e}")
            return None
    
    async def _apply_css_fixes(self, client, selector, css_fixes):
        """Apply CSS fixes to the specified component"""
        fix_js = f"""
        return new Promise((resolve) => {{
            const element = document.querySelector('{selector}');
            if (!element) {{
                resolve(JSON.stringify({{
                    success: false,
                    error: 'Element not found for CSS fixes: {selector}'
                }}));
                return;
            }}
            
            // For shadow DOM components
            if (element.shadowRoot) {{
                const styleElement = element.shadowRoot.querySelector('style');
                if (styleElement) {{
                    styleElement.textContent += `{css_fixes}`;
                    resolve(JSON.stringify({{
                        success: true,
                        message: 'CSS fixes applied to shadow DOM'
                    }}));
                    return;
                }}
            }}
            
            // For regular DOM elements - create style element
            const styleElement = document.createElement('style');
            styleElement.textContent = `{css_fixes}`;
            document.head.appendChild(styleElement);
            
            resolve(JSON.stringify({{
                success: true,
                message: 'CSS fixes applied to document head'
            }}));
        }});
        """
        
        try:
            result = await client.js.get_value(fix_js, timeout=15)
            data = json.loads(result)
            return data.get('success', False)
        except Exception as e:
            print(f"CSS fix exception: {e}")
            return False
    
    async def reload_and_verify(self, client, selector, description="final verification"):
        """Reload page and take a verification screenshot"""
        print(f"\n🔄 Reloading page for {description}...")
        
        reload_js = """
        return new Promise((resolve) => {
            window.location.reload();
            resolve(JSON.stringify({success: true}));
        });
        """
        
        await client.js.get_value(reload_js, timeout=10)
        await asyncio.sleep(3)  # Wait for page to reload
        
        print(f"📸 Taking {description} screenshot...")
        
        # Wait a bit longer for components to initialize
        wait_and_capture_js = f"""
        return new Promise((resolve) => {{
            setTimeout(() => {{
                const element = document.querySelector('{selector}');
                if (!element) {{
                    resolve(JSON.stringify({{
                        success: false,
                        error: 'Element not found after reload: {selector}'
                    }}));
                    return;
                }}
                
                html2canvas(element, {{
                    scale: 0.8,
                    useCORS: true,
                    backgroundColor: null
                }}).then(canvas => {{
                    resolve(JSON.stringify({{
                        success: true,
                        dataUrl: canvas.toDataURL('image/png', 0.9)
                    }}));
                }}).catch(error => {{
                    resolve(JSON.stringify({{
                        success: false,
                        error: 'Screenshot failed: ' + error.message
                    }}));
                }});
            }}, 1000);
        }});
        """
        
        try:
            result = await client.js.get_value(wait_and_capture_js, timeout=20)
            data = json.loads(result)
            
            if data.get('success'):
                # Save verification screenshot
                base64_data = data['dataUrl'].split(',')[1]
                image_data = base64.b64decode(base64_data)
                
                filepath = self.screenshots_dir / f"{self.component_name}_VERIFIED_{self.timestamp}.png"
                with open(filepath, 'wb') as f:
                    f.write(image_data)
                
                print(f"✅ Verification screenshot saved: {filepath}")
                return str(filepath)
            else:
                print(f"❌ Verification failed: {data.get('error')}")
                return None
                
        except Exception as e:
            print(f"Verification exception: {e}")
            return None

# Example usage functions for common scenarios
async def fix_users_agents_styling():
    """Example: Fix USERS & AGENTS section styling"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'users-agents-fixer',
            'agentName': 'Users Agents Fixer',
            'agentType': 'ai'
        })
        
        debugger = UIStylingDebugger("users_agents")
        
        css_fixes = """
            /* Search container styles */
            .search-container {
                position: relative;
                margin-bottom: 15px;
            }
            
            .search-input {
                width: 100%;
                padding: 8px 12px 8px 35px;
                background: rgba(255, 255, 255, 0.08);
                border: 1px solid rgba(255, 255, 255, 0.15);
                border-radius: 8px;
                color: #e0e6ed;
                font-size: 13px;
                box-sizing: border-box;
                transition: all 0.3s ease;
            }
            
            .search-input:focus {
                outline: none;
                border-color: rgba(0, 212, 255, 0.5);
                background: rgba(255, 255, 255, 0.12);
                box-shadow: 0 0 8px rgba(0, 212, 255, 0.2);
            }
            
            .title {
                font-size: 12px !important;
                letter-spacing: 1px !important;
                font-weight: 600 !important;
            }
            
            .favorite-star {
                display: none !important;
            }
            
            .agent-actions {
                display: flex;
                align-items: center;
                gap: 4px;
                margin-left: auto;
            }
        """
        
        result = await debugger.capture_before_after_styling_fix(
            client,
            'agent-selector',
            css_fixes,
            "USERS & AGENTS section styling fixes"
        )
        
        if result:
            # Optional: reload and verify the permanent fixes
            verified_path = await debugger.reload_and_verify(
                client, 'agent-selector', "permanent styling verification"
            )
            
            if verified_path:
                result['verified'] = verified_path
        
        return result

if __name__ == "__main__":
    # Example usage
    result = asyncio.run(fix_users_agents_styling())
    if result:
        print(f"\n🎨 UI Styling Debug Complete!")
        print(f"   Component: {result['component']}")
        print(f"   Before: {result['before']}")
        print(f"   After: {result['after']}")
        if 'verified' in result:
            print(f"   Verified: {result['verified']}")
    else:
        print("\n❌ UI styling debug failed")