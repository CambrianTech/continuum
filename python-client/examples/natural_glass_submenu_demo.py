#!/usr/bin/env python3
"""
Natural Glass Submenu Demo
🪟 Use the natural AgentSelector component that works for manual clicks
"""

import asyncio
import json
import sys
import base64
from datetime import datetime

sys.path.append('/Users/joel/Development/ideem/vHSM/externals/continuum/python-client')

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def natural_glass_submenu_demo():
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'natural-demo',
            'agentName': 'Natural Demo',
            'agentType': 'ai'
        })
        
        print("🪟 NATURAL GLASS SUBMENU DEMO")
        print("=" * 40)
        print("Using the natural AgentSelector component (the one that works for you)")
        
        # Click the natural component and capture
        natural_click_js = """
        return new Promise((resolve) => {
            console.log('🔍 Finding natural AgentSelector component...');
            
            // Find the natural component (no ID, just agent-selector)
            const selector = document.querySelector('agent-selector');
            if (!selector || !selector.shadowRoot) {
                resolve(JSON.stringify({
                    success: false,
                    error: 'No natural AgentSelector component found'
                }));
                return;
            }
            
            console.log('✅ Found natural component with shadow root');
            
            // Find Claude Code button
            const btn = selector.shadowRoot.querySelector('[data-agent-id="claude-code"] .drawer-btn');
            if (!btn) {
                resolve(JSON.stringify({
                    success: false,
                    error: 'No Claude Code button found in natural component'
                }));
                return;
            }
            
            console.log('✅ Found Claude Code button');
            
            // Get agent position before clicking
            const agentItem = selector.shadowRoot.querySelector('[data-agent-id="claude-code"]');
            const agentRect = agentItem.getBoundingClientRect();
            
            console.log('📍 Agent position:', agentRect.left, agentRect.top, agentRect.right, agentRect.bottom);
            console.log('🖱️ Clicking button...');
            
            // Click the button
            btn.click();
            
            // Wait for animation to complete
            setTimeout(() => {
                console.log('⏰ Animation complete, checking submenu...');
                
                const submenu = document.querySelector('.glass-submenu');
                if (submenu) {
                    const submenuRect = submenu.getBoundingClientRect();
                    
                    console.log('✅ Glass submenu found!');
                    console.log('📍 Submenu position:', submenuRect.left, submenuRect.top, submenuRect.right, submenuRect.bottom);
                    
                    // Take screenshot
                    html2canvas(document.body, {
                        scale: 0.7,
                        useCORS: true,
                        backgroundColor: null
                    }).then(canvas => {
                        console.log('📸 Screenshot captured');
                        
                        resolve(JSON.stringify({
                            success: true,
                            agentPosition: {
                                left: agentRect.left,
                                top: agentRect.top,
                                right: agentRect.right,
                                bottom: agentRect.bottom
                            },
                            submenuPosition: {
                                left: submenuRect.left,
                                top: submenuRect.top,
                                right: submenuRect.right,
                                bottom: submenuRect.bottom,
                                width: submenuRect.width,
                                height: submenuRect.height
                            },
                            submenuStyles: {
                                left: submenu.style.left,
                                top: submenu.style.top,
                                width: submenu.style.width,
                                opacity: submenu.style.opacity
                            },
                            viewport: {
                                width: window.innerWidth,
                                height: window.innerHeight
                            },
                            dataUrl: canvas.toDataURL('image/png', 0.9)
                        }));
                    }).catch(error => {
                        console.error('❌ Screenshot failed:', error);
                        resolve(JSON.stringify({
                            success: false,
                            error: 'Screenshot capture failed: ' + error.message
                        }));
                    });
                } else {
                    console.error('❌ No glass submenu found');
                    resolve(JSON.stringify({
                        success: false,
                        error: 'No glass submenu created after clicking'
                    }));
                }
            }, 1000);
        });
        """
        
        print("🖱️ Clicking natural component button...")
        
        result = await client.js.get_value(natural_click_js, timeout=25)
        data = json.loads(result)
        
        if data.get('success'):
            print("✅ NATURAL CLICK SUCCESSFUL!")
            
            agent_pos = data['agentPosition']
            submenu_pos = data['submenuPosition']
            submenu_styles = data['submenuStyles']
            viewport = data['viewport']
            
            print(f"\n📍 POSITIONING:")
            print(f"   Agent: left={agent_pos['left']}, top={agent_pos['top']}")
            print(f"   Submenu: left={submenu_pos['left']}, top={submenu_pos['top']}")
            print(f"   Submenu size: {submenu_pos['width']}x{submenu_pos['height']}")
            print(f"   Viewport: {viewport['width']}x{viewport['height']}")
            
            # Check if in viewport
            in_viewport = (
                submenu_pos['left'] >= 0 and 
                submenu_pos['right'] <= viewport['width'] and
                submenu_pos['top'] >= 0 and 
                submenu_pos['bottom'] <= viewport['height']
            )
            
            print(f"   In viewport: {in_viewport}")
            
            if 'dataUrl' in data:
                # Save screenshot
                base64_data = data['dataUrl'].split(',')[1]
                image_data = base64.b64decode(base64_data)
                
                timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
                filepath = f"/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/screenshots/natural_glass_submenu_{timestamp}.png"
                
                with open(filepath, 'wb') as f:
                    f.write(image_data)
                
                print(f"\n🎉 SUCCESS! Screenshot saved: {filepath}")
                return filepath
            else:
                print("❌ No screenshot data received")
                return None
        else:
            print(f"❌ FAILED: {data.get('error')}")
            return None

if __name__ == "__main__":
    result = asyncio.run(natural_glass_submenu_demo())
    if result:
        print(f"\n🪟 NATURAL GLASS SUBMENU WORKING: {result}")
    else:
        print("\n❌ Demo failed")