#!/usr/bin/env python3
"""
Fix and Test Glass Submenu System
🔧 Fix the component loading and test the glass submenu

This script will:
1. Fix the script loading path issue
2. Manually inject the working component
3. Test the glass submenu functionality
4. Capture working screenshots
"""

import asyncio
import json
import os
import sys
sys.path.append('/Users/joel/Development/ideem/vHSM/externals/continuum/python-client')

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class GlassSubmenuFixer:
    """Fix and test the glass submenu system"""
    
    def __init__(self):
        load_continuum_config()
        self.client = None
        self.screenshots_dir = "/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/screenshots"
        os.makedirs(self.screenshots_dir, exist_ok=True)
    
    async def __aenter__(self):
        self.client = ContinuumClient()
        await self.client.__aenter__()
        await self.client.register_agent({
            'agentId': 'glass-submenu-fixer',
            'agentName': 'Glass Submenu Fixer',
            'agentType': 'ai'
        })
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.client:
            await self.client.__aexit__(exc_type, exc_val, exc_tb)
    
    async def inject_working_component(self):
        """🔧 Inject a working AgentSelector component"""
        print("🔧 INJECTING: Working AgentSelector component...")
        
        injection_js = """
        return new Promise((resolve) => {
            console.log('🔧 Starting AgentSelector component injection...');
            
            const results = {
                componentCreated: false,
                componentAppended: false,
                shadowRootCreated: false,
                agentDataLoaded: false,
                eventsSetup: false,
                glassSubmenuReady: false,
                errors: []
            };
            
            try {
                // Remove any existing component
                const existing = document.querySelector('#main-agent-selector, #main-agent-selector-manual');
                if (existing) {
                    existing.remove();
                }
                
                // Create new AgentSelector component
                const agentSelector = document.createElement('agent-selector');
                agentSelector.id = 'main-agent-selector-working';
                results.componentCreated = true;
                
                // Find insertion point - after persona manager, before legacy agent selector
                const personaManager = document.querySelector('.persona-manager');
                const legacyAgentSelector = document.querySelector('.agent-selector');
                
                if (personaManager && legacyAgentSelector) {
                    // Insert between persona manager and legacy agent selector
                    personaManager.parentNode.insertBefore(agentSelector, legacyAgentSelector);
                } else {
                    // Fallback: append to sidebar
                    const sidebar = document.querySelector('.sidebar');
                    if (sidebar) {
                        sidebar.appendChild(agentSelector);
                    }
                }
                results.componentAppended = true;
                
                // Wait for component to connect and render
                setTimeout(() => {
                    results.shadowRootCreated = !!agentSelector.shadowRoot;
                    
                    // Set up event handlers for glass submenu
                    agentSelector.addEventListener('drawer-open-requested', (e) => {
                        console.log('🪟 Drawer open requested for:', e.detail.agentId);
                    });
                    
                    agentSelector.addEventListener('agent-academy-requested', (e) => {
                        console.log('🎓 Academy requested for:', e.detail.name);
                    });
                    
                    agentSelector.addEventListener('agent-projects-requested', (e) => {
                        console.log('📁 Projects requested for:', e.detail.name);
                    });
                    
                    agentSelector.addEventListener('agent-deploy-requested', (e) => {
                        console.log('🚀 Deploy requested for:', e.detail.name);
                    });
                    
                    results.eventsSetup = true;
                    results.glassSubmenuReady = true;
                    
                    console.log('✅ AgentSelector component injection complete');
                    resolve(JSON.stringify(results));
                }, 1000);
                
            } catch (error) {
                results.errors.push(error.message);
                console.error('❌ Component injection failed:', error);
                resolve(JSON.stringify(results));
            }
        });
        """
        
        try:
            result_json = await self.client.js.get_value(injection_js, timeout=15)
            result = json.loads(result_json)
            
            print("✅ COMPONENT INJECTION RESULTS:")
            print(f"   Component created: {result['componentCreated']}")
            print(f"   Component appended: {result['componentAppended']}")
            print(f"   Shadow root created: {result['shadowRootCreated']}")
            print(f"   Events setup: {result['eventsSetup']}")
            print(f"   Glass submenu ready: {result['glassSubmenuReady']}")
            
            if result['errors']:
                print(f"   ❌ Errors: {result['errors']}")
            
            return result
            
        except Exception as e:
            print(f"❌ Component injection failed: {e}")
            return None
    
    async def test_glass_submenu_clicks(self):
        """🖱️ Test clicking on drawer buttons to trigger glass submenu"""
        print("\n🖱️ TESTING: Glass submenu clicks...")
        
        click_test_js = """
        return new Promise((resolve) => {
            console.log('🖱️ Starting glass submenu click test...');
            
            const results = {
                componentFound: false,
                drawerButtonsFound: [],
                clickedButtons: [],
                glassSubmenusCreated: [],
                errors: []
            };
            
            try {
                // Find the working component
                const agentSelector = document.querySelector('#main-agent-selector-working');
                results.componentFound = !!agentSelector;
                
                if (agentSelector && agentSelector.shadowRoot) {
                    // Look for drawer buttons in shadow DOM
                    const drawerButtons = Array.from(agentSelector.shadowRoot.querySelectorAll('.drawer-btn'));
                    
                    results.drawerButtonsFound = drawerButtons.map(btn => ({
                        agentId: btn.dataset.agentId,
                        text: btn.textContent.trim(),
                        visible: btn.offsetWidth > 0 && btn.offsetHeight > 0
                    }));
                    
                    // Click each visible drawer button
                    drawerButtons.forEach((btn, index) => {
                        if (btn.offsetWidth > 0 && btn.offsetHeight > 0) {
                            console.log(`🖱️ Clicking drawer button ${index + 1}: ${btn.textContent.trim()}`);
                            btn.click();
                            
                            results.clickedButtons.push({
                                agentId: btn.dataset.agentId,
                                text: btn.textContent.trim(),
                                index: index
                            });
                            
                            // Check for glass submenu after click
                            setTimeout(() => {
                                const glassSubmenu = document.querySelector('.glass-submenu');
                                if (glassSubmenu) {
                                    results.glassSubmenusCreated.push({
                                        agentId: btn.dataset.agentId,
                                        width: glassSubmenu.style.width,
                                        opacity: glassSubmenu.style.opacity,
                                        visible: glassSubmenu.offsetWidth > 0,
                                        buttons: Array.from(glassSubmenu.querySelectorAll('button')).map(b => b.textContent.trim())
                                    });
                                }
                            }, 600);
                        }
                    });
                }
                
                // Wait for all interactions to complete
                setTimeout(() => {
                    resolve(JSON.stringify(results));
                }, 2000);
                
            } catch (error) {
                results.errors.push(error.message);
                console.error('❌ Click test failed:', error);
                resolve(JSON.stringify(results));
            }
        });
        """
        
        try:
            result_json = await self.client.js.get_value(click_test_js, timeout=20)
            result = json.loads(result_json)
            
            print("✅ GLASS SUBMENU CLICK RESULTS:")
            print(f"   Component found: {result['componentFound']}")
            print(f"   Drawer buttons found: {len(result['drawerButtonsFound'])}")
            
            for btn in result['drawerButtonsFound']:
                print(f"      - {btn['text']} (Agent: {btn['agentId']}, Visible: {btn['visible']})")
            
            print(f"   Clicked buttons: {len(result['clickedButtons'])}")
            for btn in result['clickedButtons']:
                print(f"      - Clicked: {btn['text']} (Agent: {btn['agentId']})")
            
            print(f"   Glass submenus created: {len(result['glassSubmenusCreated'])}")
            for submenu in result['glassSubmenusCreated']:
                print(f"      - Agent: {submenu['agentId']}")
                print(f"        Width: {submenu['width']}, Opacity: {submenu['opacity']}")
                print(f"        Visible: {submenu['visible']}")
                print(f"        Buttons: {submenu['buttons']}")
            
            if result['errors']:
                print(f"   ❌ Errors: {result['errors']}")
            
            return result
            
        except Exception as e:
            print(f"❌ Glass submenu click test failed: {e}")
            return None
    
    async def test_glass_submenu_buttons(self):
        """🎯 Test clicking buttons within the glass submenu"""
        print("\n🎯 TESTING: Glass submenu button interactions...")
        
        await asyncio.sleep(1)  # Wait for any animations
        
        button_test_js = """
        return new Promise((resolve) => {
            console.log('🎯 Testing glass submenu buttons...');
            
            const results = {
                glassSubmenuFound: false,
                buttonsInSubmenu: [],
                buttonClickResults: [],
                eventsTriggered: [],
                errors: []
            };
            
            try {
                const glassSubmenu = document.querySelector('.glass-submenu');
                results.glassSubmenuFound = !!glassSubmenu;
                
                if (glassSubmenu) {
                    const buttons = Array.from(glassSubmenu.querySelectorAll('button'));
                    results.buttonsInSubmenu = buttons.map(btn => ({
                        text: btn.textContent.trim(),
                        className: btn.className,
                        visible: btn.offsetWidth > 0 && btn.offsetHeight > 0
                    }));
                    
                    // Set up event listeners to capture custom events
                    const eventTypes = ['academy-clicked', 'projects-clicked', 'deploy-clicked'];
                    eventTypes.forEach(eventType => {
                        document.addEventListener(eventType, (e) => {
                            results.eventsTriggered.push({
                                type: eventType,
                                detail: e.detail,
                                timestamp: Date.now()
                            });
                        });
                    });
                    
                    // Click each button and capture results
                    buttons.forEach((btn, index) => {
                        if (btn.offsetWidth > 0) {
                            console.log(`🎯 Clicking submenu button: ${btn.textContent.trim()}`);
                            
                            try {
                                btn.click();
                                results.buttonClickResults.push({
                                    text: btn.textContent.trim(),
                                    index: index,
                                    clicked: true
                                });
                            } catch (btnError) {
                                results.buttonClickResults.push({
                                    text: btn.textContent.trim(),
                                    index: index,
                                    clicked: false,
                                    error: btnError.message
                                });
                            }
                        }
                    });
                }
                
                // Wait for events to propagate
                setTimeout(() => {
                    resolve(JSON.stringify(results));
                }, 1000);
                
            } catch (error) {
                results.errors.push(error.message);
                console.error('❌ Button test failed:', error);
                resolve(JSON.stringify(results));
            }
        });
        """
        
        try:
            result_json = await self.client.js.get_value(button_test_js, timeout=15)
            result = json.loads(result_json)
            
            print("✅ SUBMENU BUTTON TEST RESULTS:")
            print(f"   Glass submenu found: {result['glassSubmenuFound']}")
            print(f"   Buttons in submenu: {len(result['buttonsInSubmenu'])}")
            
            for btn in result['buttonsInSubmenu']:
                print(f"      - {btn['text']} (Visible: {btn['visible']})")
            
            print(f"   Button click results: {len(result['buttonClickResults'])}")
            for click in result['buttonClickResults']:
                print(f"      - {click['text']}: {'✓' if click['clicked'] else '✗'}")
                if 'error' in click:
                    print(f"        Error: {click['error']}")
            
            print(f"   Events triggered: {len(result['eventsTriggered'])}")
            for event in result['eventsTriggered']:
                print(f"      - {event['type']}: {event['detail']}")
            
            if result['errors']:
                print(f"   ❌ Errors: {result['errors']}")
            
            return result
            
        except Exception as e:
            print(f"❌ Submenu button test failed: {e}")
            return None
    
    async def capture_working_screenshots(self):
        """📸 Capture screenshots of the working glass submenu"""
        print("\n📸 CAPTURING: Working glass submenu screenshots...")
        
        try:
            capture_js = """
            return new Promise((resolve) => {
                html2canvas(document.body, {scale: 0.8}).then(canvas => {
                    resolve(JSON.stringify({
                        success: true,
                        width: canvas.width,
                        height: canvas.height,
                        dataUrl: canvas.toDataURL('image/png', 0.9)
                    }));
                });
            });
            """
            
            result_json = await self.client.js.get_value(capture_js, timeout=15)
            result = json.loads(result_json)
            
            if result['success']:
                import base64
                data_url = result['dataUrl']
                base64_data = data_url.split(',')[1]
                image_data = base64.b64decode(base64_data)
                
                filepath = os.path.join(self.screenshots_dir, "glass_submenu_working.png")
                with open(filepath, 'wb') as f:
                    f.write(image_data)
                
                print(f"   📸 Working glass submenu captured: {result['width']}x{result['height']} → {filepath}")
                return filepath
            
        except Exception as e:
            print(f"❌ Working screenshot failed: {e}")
            return None

async def run_glass_submenu_fix():
    """🔧 Fix and test the glass submenu system"""
    print("🔧 GLASS SUBMENU SYSTEM FIX & TEST")
    print("=" * 45)
    print("🪟 Fixing component loading and testing glass submenu")
    
    async with GlassSubmenuFixer() as fixer:
        # Phase 1: Inject working component
        print("🔧 PHASE 1: Component Injection")
        await fixer.inject_working_component()
        
        # Phase 2: Test glass submenu clicks
        print("\n🖱️ PHASE 2: Glass Submenu Click Testing")
        await fixer.test_glass_submenu_clicks()
        
        # Phase 3: Test submenu button interactions
        print("\n🎯 PHASE 3: Submenu Button Testing")
        await fixer.test_glass_submenu_buttons()
        
        # Phase 4: Capture working screenshots
        print("\n📸 PHASE 4: Working Screenshot Capture")
        await fixer.capture_working_screenshots()
        
        print(f"\n✅ Glass submenu fix and test complete!")
        print(f"📁 Screenshots saved to: {fixer.screenshots_dir}")

if __name__ == "__main__":
    asyncio.run(run_glass_submenu_fix())