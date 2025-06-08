#!/usr/bin/env python3
"""
Capture full UI screenshot for debugging
Handles scroll and viewport issues to get complete UI capture
"""
import asyncio
import json
import sys
import base64
from pathlib import Path
from datetime import datetime

# Add python-client to path
script_dir = Path(__file__).parent
client_dir = script_dir.parent.parent.parent / 'python-client'
sys.path.append(str(client_dir))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def capture_full_ui_screenshot():
    """Capture full UI screenshot with proper viewport handling"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'screenshot-capturer',
            'agentName': 'Full UI Screenshot Capturer',
            'agentType': 'ai'
        })
        
        print("📸 CAPTURING FULL UI SCREENSHOT")
        print("=" * 35)
        
        # First, get current viewport and scroll info
        viewport_check_js = """
        return new Promise((resolve) => {
            const viewportInfo = {
                windowWidth: window.innerWidth,
                windowHeight: window.innerHeight,
                documentWidth: document.documentElement.scrollWidth,
                documentHeight: document.documentElement.scrollHeight,
                scrollX: window.scrollX,
                scrollY: window.scrollY,
                devicePixelRatio: window.devicePixelRatio || 1
            };
            
            // Check for sidebar and main content dimensions
            const sidebar = document.querySelector('.sidebar');
            const mainContent = document.querySelector('.main-content');
            
            if (sidebar) {
                viewportInfo.sidebarWidth = sidebar.offsetWidth;
                viewportInfo.sidebarHeight = sidebar.offsetHeight;
            }
            
            if (mainContent) {
                viewportInfo.mainContentWidth = mainContent.offsetWidth;
                viewportInfo.mainContentHeight = mainContent.offsetHeight;
            }
            
            // Check for duplicate agent selectors
            const agentSelectors = document.querySelectorAll('agent-selector, .agent-selector');
            viewportInfo.agentSelectorCount = agentSelectors.length;
            viewportInfo.agentSelectorInfo = Array.from(agentSelectors).map((el, index) => ({
                index: index,
                tagName: el.tagName,
                className: el.className,
                id: el.id,
                offsetTop: el.offsetTop,
                offsetLeft: el.offsetLeft,
                offsetWidth: el.offsetWidth,
                offsetHeight: el.offsetHeight,
                visible: el.offsetWidth > 0 && el.offsetHeight > 0,
                hasContent: el.innerHTML.length > 0
            }));
            
            resolve(JSON.stringify({
                success: true,
                viewport: viewportInfo
            }));
        });
        """
        
        print("🔍 Checking viewport and UI layout...")
        viewport_result = await client.js.get_value(viewport_check_js, timeout=10)
        viewport_data = json.loads(viewport_result)
        
        if viewport_data.get('success'):
            viewport = viewport_data.get('viewport', {})
            print(f"📊 Viewport: {viewport.get('windowWidth')}x{viewport.get('windowHeight')}")
            print(f"📊 Document: {viewport.get('documentWidth')}x{viewport.get('documentHeight')}")
            print(f"📊 Scroll: ({viewport.get('scrollX')}, {viewport.get('scrollY')})")
            print(f"📊 Device Pixel Ratio: {viewport.get('devicePixelRatio')}")
            
            if viewport.get('sidebarWidth'):
                print(f"📊 Sidebar: {viewport.get('sidebarWidth')}x{viewport.get('sidebarHeight')}")
            
            # Check for duplicate agent selectors
            agent_count = viewport.get('agentSelectorCount', 0)
            print(f"\n🔍 Agent Selector Analysis:")
            print(f"   • Found {agent_count} agent selector elements")
            
            if agent_count > 1:
                print(f"⚠️  DUPLICATE AGENT SELECTORS DETECTED!")
                agent_info = viewport.get('agentSelectorInfo', [])
                for info in agent_info:
                    print(f"   {info['index']+1}. {info['tagName']} - {info['className']}")
                    print(f"      Position: ({info['offsetLeft']}, {info['offsetTop']})")
                    print(f"      Size: {info['offsetWidth']}x{info['offsetHeight']}")
                    print(f"      Visible: {info['visible']}, Has Content: {info['hasContent']}")
        
        # Scroll to top to ensure we capture from the beginning
        scroll_to_top_js = """
        return new Promise((resolve) => {
            window.scrollTo(0, 0);
            setTimeout(() => {
                resolve(JSON.stringify({success: true}));
            }, 500); // Wait for smooth scroll to complete
        });
        """
        
        print(f"\n📜 Scrolling to top for full capture...")
        await client.js.get_value(scroll_to_top_js, timeout=5)
        
        # Take screenshot with full page capture
        print(f"📸 Capturing full UI screenshot...")
        
        # Use the client's screenshot capability
        try:
            screenshot_data = await client.screenshot()
            
            # Save screenshot with timestamp
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            screenshot_path = f"agents/workspace/ui-debugging/screenshots/full_ui_{timestamp}.png"
            
            # Create screenshots directory if it doesn't exist
            screenshots_dir = Path("agents/workspace/ui-debugging/screenshots")
            screenshots_dir.mkdir(parents=True, exist_ok=True)
            
            # Decode and save screenshot
            if isinstance(screenshot_data, str):
                # If it's base64 encoded
                screenshot_bytes = base64.b64decode(screenshot_data)
            else:
                screenshot_bytes = screenshot_data
            
            with open(screenshot_path, 'wb') as f:
                f.write(screenshot_bytes)
            
            print(f"✅ Screenshot saved: {screenshot_path}")
            print(f"📁 Full path: {Path(screenshot_path).absolute()}")
            
            # Get file size
            file_size = Path(screenshot_path).stat().st_size
            print(f"📊 File size: {file_size:,} bytes ({file_size/1024:.1f} KB)")
            
            return screenshot_path
            
        except Exception as e:
            print(f"❌ Screenshot capture failed: {e}")
            
            # Try alternative method using browser screenshot command
            print(f"🔄 Trying alternative screenshot method...")
            
            try:
                # Use client screenshot command
                screenshot_result = await client.send_message({
                    'type': 'screenshot',
                    'timestamp': datetime.now().isoformat()
                })
                
                if screenshot_result:
                    print(f"✅ Alternative screenshot method succeeded")
                    return "screenshot_taken_via_alternative_method"
                
            except Exception as e2:
                print(f"❌ Alternative screenshot method also failed: {e2}")
                return None

async def main():
    """Main function"""
    print("📸 Full UI Screenshot Capturer")
    print("=" * 30)
    print()
    
    try:
        screenshot_path = await capture_full_ui_screenshot()
        
        if screenshot_path:
            print(f"\n" + "=" * 35)
            print(f"✅ SCREENSHOT CAPTURE COMPLETE")
            print(f"=" * 35)
            print()
            print(f"📁 Screenshot location: {screenshot_path}")
            print()
            print(f"💡 Next Steps:")
            print(f"   • Open screenshot to analyze UI layout")
            print(f"   • Look for duplicate agent selector elements")
            print(f"   • Check sidebar layout and positioning")
            print(f"   • Verify component alignment and spacing")
        else:
            print(f"\n" + "=" * 35)
            print(f"❌ SCREENSHOT CAPTURE FAILED")
            print(f"=" * 35)
            print()
            print(f"🔧 Troubleshooting:")
            print(f"   • Ensure Continuum server is running")
            print(f"   • Check browser is open and responsive")
            print(f"   • Verify WebSocket connection is working")
            print(f"   • Try using browser dev tools to take screenshot manually")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        print(f"🔧 Troubleshooting:")
        print(f"   • Ensure Continuum server is running")
        print(f"   • Check WebSocket connection")
        print(f"   • Verify browser has active Continuum tab")

if __name__ == "__main__":
    asyncio.run(main())