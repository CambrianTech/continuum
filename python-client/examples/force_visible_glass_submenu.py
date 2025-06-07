#!/usr/bin/env python3
"""
Force Visible Glass Submenu
🪟 Scroll to make agent visible, click button, capture submenu opening
"""

import asyncio
import json
import sys
import base64
from datetime import datetime

sys.path.append('/Users/joel/Development/ideem/vHSM/externals/continuum/python-client')

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def force_visible_glass_submenu():
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'force-visible',
            'agentName': 'Force Visible',
            'agentType': 'ai'
        })
        
        print("🔧 STEP 1: Setting up and positioning...")
        
        # Set up component and scroll to make agents visible in viewport
        setup_js = """
        return new Promise((resolve) => {
            console.log('🔧 Setting up visible glass submenu test...');
            
            // Remove existing component
            const existing = document.querySelector('#main-agent-selector-working');
            if (existing) existing.remove();
            
            // Create component
            const agentSelector = document.createElement('agent-selector');
            agentSelector.id = 'main-agent-selector-working';
            
            // Add to sidebar
            const sidebar = document.querySelector('.sidebar');
            if (sidebar) {
                sidebar.appendChild(agentSelector);
                
                // Wait for component to initialize
                setTimeout(() => {
                    // Scroll the agents section into view
                    const usersSection = sidebar.querySelector('h3');
                    if (usersSection) {
                        usersSection.scrollIntoView({behavior: 'smooth', block: 'start'});
                        console.log('📍 Scrolled agents section into view');
                    }
                    
                    // Wait a bit more for scroll
                    setTimeout(() => {
                        resolve(JSON.stringify({success: true}));
                    }, 500);
                }, 1000);
            } else {
                resolve(JSON.stringify({success: false, error: 'No sidebar'}));
            }
        });
        """
        
        setup_result = await client.js.get_value(setup_js, timeout=15)
        setup_data = json.loads(setup_result)
        
        if not setup_data.get('success'):
            print(f"❌ Setup failed: {setup_data.get('error')}")
            return
        
        print("✅ Setup complete, agents positioned in viewport")
        
        print("\n🖱️ STEP 2: Clicking and forcing submenu into viewport...")
        
        # Click and modify positioning to force into viewport
        click_and_fix_js = """
        return new Promise((resolve) => {
            console.log('🖱️ Starting click and fix positioning...');
            
            const working = document.querySelector('#main-agent-selector-working');
            if (!working || !working.shadowRoot) {
                resolve(JSON.stringify({success: false, error: 'Component not ready'}));
                return;
            }
            
            const btn = working.shadowRoot.querySelector('[data-agent-id="claude-code"] .drawer-btn');
            if (!btn) {
                resolve(JSON.stringify({success: false, error: 'Button not found'}));
                return;
            }
            
            console.log('✅ Found button, clicking now...');
            btn.click();
            
            // Wait for submenu to be created
            setTimeout(() => {
                const submenu = document.querySelector('.glass-submenu');
                if (submenu) {
                    console.log('✅ Glass submenu created, fixing position...');
                    
                    // Force submenu into visible area
                    submenu.style.position = 'fixed';
                    submenu.style.left = '350px';
                    submenu.style.top = '400px';
                    submenu.style.zIndex = '999999';
                    
                    console.log('📍 Submenu repositioned to visible area');
                    console.log('🪟 Submenu styles:', {
                        left: submenu.style.left,
                        top: submenu.style.top,
                        width: submenu.style.width,
                        opacity: submenu.style.opacity,
                        zIndex: submenu.style.zIndex
                    });
                    
                    resolve(JSON.stringify({
                        success: true,
                        submenuVisible: true,
                        position: {
                            left: submenu.style.left,
                            top: submenu.style.top,
                            width: submenu.style.width,
                            opacity: submenu.style.opacity
                        }
                    }));
                } else {
                    console.error('❌ No glass submenu found after click');
                    resolve(JSON.stringify({success: false, error: 'No submenu created'}));
                }
            }, 800);
        });
        """
        
        click_result = await client.js.get_value(click_and_fix_js, timeout=20)
        click_data = json.loads(click_result)
        
        if not click_data.get('success'):
            print(f"❌ Click failed: {click_data.get('error')}")
            return
        
        print("✅ GLASS SUBMENU IS NOW VISIBLE!")
        print(f"   Position: {click_data.get('position')}")
        
        print("\n📸 STEP 3: Capturing the visible glass submenu...")
        
        # Capture screenshot
        capture_js = """
        return new Promise((resolve) => {
            console.log('📸 Capturing visible glass submenu...');
            
            html2canvas(document.body, {
                scale: 0.7,
                useCORS: true,
                backgroundColor: null
            }).then(canvas => {
                console.log('✅ Capture complete:', canvas.width, 'x', canvas.height);
                resolve(canvas.toDataURL('image/png', 0.9));
            }).catch(error => {
                console.error('❌ Capture failed:', error);
                resolve('ERROR: ' + error.message);
            });
        });
        """
        
        screenshot = await client.js.get_value(capture_js, timeout=15)
        
        if screenshot.startswith('data:image'):
            # Save screenshot
            base64_data = screenshot.split(',')[1]
            image_data = base64.b64decode(base64_data)
            
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            filepath = f"/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/screenshots/glass_submenu_VISIBLE_{timestamp}.png"
            
            with open(filepath, 'wb') as f:
                f.write(image_data)
            
            print(f"🎉 VISIBLE GLASS SUBMENU CAPTURED: {filepath}")
            return filepath
        else:
            print(f"❌ Screenshot failed: {screenshot}")
            return None

if __name__ == "__main__":
    result = asyncio.run(force_visible_glass_submenu())
    if result:
        print(f"\n🪟 MONEY SHOT: {result}")
    else:
        print("\n❌ Still broken - no visible submenu captured")