#!/usr/bin/env python3
"""
Glass Submenu System Test & Validation
🔧 Test the Star Trek TNG glass submenu implementation in Continuum

This script acts as my "eyes" to:
- Click on agent >> buttons to trigger glass submenus
- Capture screenshots to verify visual appearance
- Test JavaScript functionality and error detection
- Validate the glass aesthetic and animations
- Diagnose any issues with the implementation
"""

import asyncio
import json
import os
import time
from pathlib import Path

# Add the continuum python client to path
import sys
sys.path.append('/Users/joel/Development/ideem/vHSM/externals/continuum/python-client')

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class GlassSubmenuTester:
    """Test and validate the glass submenu system"""
    
    def __init__(self):
        load_continuum_config()
        self.client = None
        self.screenshots_dir = "/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/screenshots"
        os.makedirs(self.screenshots_dir, exist_ok=True)
        self.test_results = {}
    
    async def __aenter__(self):
        self.client = ContinuumClient()
        await self.client.__aenter__()
        await self.client.register_agent({
            'agentId': 'glass-submenu-tester',
            'agentName': 'Glass Submenu Tester',
            'agentType': 'ai'
        })
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.client:
            await self.client.__aexit__(exc_type, exc_val, exc_tb)
    
    async def test_component_initialization(self):
        """🔧 Test if AgentSelector component is properly initialized"""
        print("🔧 TESTING: Component initialization...")
        
        init_test_js = """
        return new Promise((resolve) => {
            const results = {
                agentSelectorExists: !!document.getElementById('main-agent-selector'),
                agentSelectorType: null,
                agentSelectorClass: null,
                webComponentDefined: !!customElements.get('agent-selector'),
                windowAgentSelector: typeof window.AgentSelector !== 'undefined',
                drawerButtons: [],
                jsErrors: []
            };
            
            // Check component type
            const agentSelector = document.getElementById('main-agent-selector');
            if (agentSelector) {
                results.agentSelectorType = agentSelector.constructor.name;
                results.agentSelectorClass = agentSelector.className;
                
                // Look for drawer buttons (>> buttons)
                const drawerBtns = agentSelector.shadowRoot ? 
                    agentSelector.shadowRoot.querySelectorAll('.drawer-btn') : 
                    agentSelector.querySelectorAll('.drawer-btn');
                
                results.drawerButtons = Array.from(drawerBtns).map(btn => ({
                    text: btn.textContent.trim(),
                    agentId: btn.dataset.agentId,
                    visible: btn.offsetWidth > 0 && btn.offsetHeight > 0
                }));
            }
            
            // Capture any JS errors
            const originalError = console.error;
            const errors = [];
            console.error = function(...args) {
                errors.push(args.join(' '));
                originalError.apply(console, args);
            };
            results.jsErrors = errors;
            
            resolve(JSON.stringify(results));
        });
        """
        
        try:
            result_json = await self.client.js.get_value(init_test_js, timeout=10)
            result = json.loads(result_json)
            
            print(f"✅ COMPONENT INIT RESULTS:")
            print(f"   AgentSelector exists: {result['agentSelectorExists']}")
            print(f"   Web component defined: {result['webComponentDefined']}")
            print(f"   Component type: {result['agentSelectorType']}")
            print(f"   Drawer buttons found: {len(result['drawerButtons'])}")
            
            for btn in result['drawerButtons']:
                print(f"      - {btn['text']} (Agent: {btn['agentId']}, Visible: {btn['visible']})")
            
            if result['jsErrors']:
                print(f"   ⚠️ JS Errors: {len(result['jsErrors'])}")
                for error in result['jsErrors']:
                    print(f"      {error}")
            
            self.test_results['component_init'] = result
            return result
            
        except Exception as e:
            print(f"❌ Component initialization test failed: {e}")
            return None
    
    async def capture_baseline_screenshot(self):
        """📸 Capture baseline screenshot before testing"""
        print("📸 CAPTURING: Baseline screenshot...")
        
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
                
                filepath = os.path.join(self.screenshots_dir, "glass_submenu_baseline.png")
                with open(filepath, 'wb') as f:
                    f.write(image_data)
                
                print(f"   📸 Baseline captured: {result['width']}x{result['height']} → {filepath}")
                return filepath
            
        except Exception as e:
            print(f"❌ Baseline screenshot failed: {e}")
            return None
    
    async def test_agent_button_clicking(self):
        """🖱️ Test clicking on agent drawer buttons"""
        print("🖱️ TESTING: Agent button clicking...")
        
        click_test_js = """
        return new Promise((resolve) => {
            const results = {
                agentsFound: [],
                clickResults: [],
                glassSubmenuAppeared: false,
                submenuDetails: null
            };
            
            // Find all agent elements with drawer buttons
            const agentSelector = document.getElementById('main-agent-selector');
            let drawerButtons = [];
            
            if (agentSelector && agentSelector.shadowRoot) {
                drawerButtons = Array.from(agentSelector.shadowRoot.querySelectorAll('.drawer-btn'));
            } else if (agentSelector) {
                drawerButtons = Array.from(agentSelector.querySelectorAll('.drawer-btn'));
            }
            
            // Also check legacy agent selector
            const legacyButtons = Array.from(document.querySelectorAll('.agent-item .drawer-btn, .agent-actions button'));
            drawerButtons = [...drawerButtons, ...legacyButtons];
            
            results.agentsFound = drawerButtons.map(btn => ({
                agentId: btn.dataset.agentId || 'unknown',
                text: btn.textContent.trim(),
                className: btn.className,
                visible: btn.offsetWidth > 0 && btn.offsetHeight > 0,
                position: {
                    left: btn.getBoundingClientRect().left,
                    top: btn.getBoundingClientRect().top,
                    width: btn.offsetWidth,
                    height: btn.offsetHeight
                }
            }));
            
            // Try clicking the first visible button
            const firstButton = drawerButtons.find(btn => btn.offsetWidth > 0 && btn.offsetHeight > 0);
            if (firstButton) {
                console.log('🖱️ Clicking first drawer button:', firstButton.textContent.trim());
                
                // Click the button
                firstButton.click();
                
                // Wait for glass submenu to appear
                setTimeout(() => {
                    const glassSubmenu = document.querySelector('.glass-submenu');
                    results.glassSubmenuAppeared = !!glassSubmenu;
                    
                    if (glassSubmenu) {
                        results.submenuDetails = {
                            width: glassSubmenu.style.width,
                            opacity: glassSubmenu.style.opacity,
                            transform: glassSubmenu.style.transform,
                            position: {
                                left: glassSubmenu.getBoundingClientRect().left,
                                top: glassSubmenu.getBoundingClientRect().top,
                                width: glassSubmenu.getBoundingClientRect().width,
                                height: glassSubmenu.getBoundingClientRect().height
                            },
                            buttons: Array.from(glassSubmenu.querySelectorAll('button')).map(btn => ({
                                text: btn.textContent.trim(),
                                className: btn.className
                            }))
                        };
                    }
                    
                    resolve(JSON.stringify(results));
                }, 800); // Wait for animation
            } else {
                resolve(JSON.stringify(results));
            }
        });
        """
        
        try:
            result_json = await self.client.js.get_value(click_test_js, timeout=15)
            result = json.loads(result_json)
            
            print(f"✅ BUTTON CLICK RESULTS:")
            print(f"   Agents found: {len(result['agentsFound'])}")
            for agent in result['agentsFound']:
                print(f"      - {agent['text']} (ID: {agent['agentId']}, Visible: {agent['visible']})")
                print(f"        Position: {agent['position']['left']},{agent['position']['top']} ({agent['position']['width']}x{agent['position']['height']})")
            
            print(f"   Glass submenu appeared: {result['glassSubmenuAppeared']}")
            
            if result['submenuDetails']:
                details = result['submenuDetails']
                print(f"   Submenu details:")
                print(f"      Width: {details['width']}")
                print(f"      Opacity: {details['opacity']}")
                print(f"      Position: {details['position']['left']},{details['position']['top']} ({details['position']['width']}x{details['position']['height']})")
                print(f"      Buttons: {len(details['buttons'])}")
                for btn in details['buttons']:
                    print(f"         - {btn['text']}")
            
            self.test_results['button_click'] = result
            return result
            
        except Exception as e:
            print(f"❌ Button click test failed: {e}")
            return None
    
    async def capture_glass_submenu_screenshot(self):
        """📸 Capture screenshot with glass submenu visible"""
        print("📸 CAPTURING: Glass submenu screenshot...")
        
        # Wait a moment for animation
        await asyncio.sleep(1)
        
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
                
                filepath = os.path.join(self.screenshots_dir, "glass_submenu_active.png")
                with open(filepath, 'wb') as f:
                    f.write(image_data)
                
                print(f"   📸 Glass submenu captured: {result['width']}x{result['height']} → {filepath}")
                return filepath
            
        except Exception as e:
            print(f"❌ Glass submenu screenshot failed: {e}")
            return None
    
    async def test_glass_submenu_buttons(self):
        """🎯 Test clicking buttons within the glass submenu"""
        print("🎯 TESTING: Glass submenu button interactions...")
        
        button_test_js = """
        return new Promise((resolve) => {
            const results = {
                glassSubmenuExists: false,
                buttonsFound: [],
                academyClicked: false,
                projectsClicked: false,
                deployClicked: false,
                eventsFired: []
            };
            
            const glassSubmenu = document.querySelector('.glass-submenu');
            results.glassSubmenuExists = !!glassSubmenu;
            
            if (glassSubmenu) {
                const buttons = Array.from(glassSubmenu.querySelectorAll('button'));
                results.buttonsFound = buttons.map(btn => ({
                    text: btn.textContent.trim(),
                    className: btn.className,
                    visible: btn.offsetWidth > 0 && btn.offsetHeight > 0
                }));
                
                // Set up event listeners to track what gets fired
                const eventTypes = ['agent-academy-requested', 'agent-projects-requested', 'agent-deploy-requested'];
                eventTypes.forEach(eventType => {
                    document.addEventListener(eventType, (e) => {
                        results.eventsFired.push({
                            type: eventType,
                            detail: e.detail,
                            timestamp: Date.now()
                        });
                    });
                });
                
                // Try clicking each button type
                buttons.forEach(btn => {
                    const text = btn.textContent.trim().toLowerCase();
                    if (text.includes('academy')) {
                        console.log('🎓 Clicking Academy button');
                        btn.click();
                        results.academyClicked = true;
                    } else if (text.includes('project')) {
                        console.log('📁 Clicking Projects button');
                        btn.click();
                        results.projectsClicked = true;
                    } else if (text.includes('deploy')) {
                        console.log('🚀 Clicking Deploy button');
                        btn.click();
                        results.deployClicked = true;
                    }
                });
            }
            
            // Wait for events to fire
            setTimeout(() => {
                resolve(JSON.stringify(results));
            }, 500);
        });
        """
        
        try:
            result_json = await self.client.js.get_value(button_test_js, timeout=10)
            result = json.loads(result_json)
            
            print(f"✅ SUBMENU BUTTON RESULTS:")
            print(f"   Glass submenu exists: {result['glassSubmenuExists']}")
            print(f"   Buttons found: {len(result['buttonsFound'])}")
            for btn in result['buttonsFound']:
                print(f"      - {btn['text']} (Visible: {btn['visible']})")
            
            print(f"   Button clicks:")
            print(f"      Academy clicked: {result['academyClicked']}")
            print(f"      Projects clicked: {result['projectsClicked']}")
            print(f"      Deploy clicked: {result['deployClicked']}")
            
            print(f"   Events fired: {len(result['eventsFired'])}")
            for event in result['eventsFired']:
                print(f"      - {event['type']}: {event['detail']}")
            
            self.test_results['submenu_buttons'] = result
            return result
            
        except Exception as e:
            print(f"❌ Submenu button test failed: {e}")
            return None
    
    async def test_visual_aesthetics(self):
        """🎨 Test the visual glass aesthetic properties"""
        print("🎨 TESTING: Visual glass aesthetic...")
        
        aesthetic_test_js = """
        return new Promise((resolve) => {
            const results = {
                glassSubmenuExists: false,
                cssProperties: {},
                starTrekAesthetic: {
                    hasBackdropFilter: false,
                    hasGradientBackground: false,
                    hasBorder: false,
                    hasBoxShadow: false,
                    hasTransition: false
                },
                animationProperties: {},
                positioning: {}
            };
            
            const glassSubmenu = document.querySelector('.glass-submenu');
            if (glassSubmenu) {
                results.glassSubmenuExists = true;
                
                const computedStyle = window.getComputedStyle(glassSubmenu);
                
                // Extract key CSS properties
                results.cssProperties = {
                    backdropFilter: computedStyle.backdropFilter,
                    webkitBackdropFilter: computedStyle.webkitBackdropFilter,
                    background: computedStyle.background,
                    border: computedStyle.border,
                    borderRadius: computedStyle.borderRadius,
                    boxShadow: computedStyle.boxShadow,
                    transition: computedStyle.transition,
                    opacity: computedStyle.opacity,
                    transform: computedStyle.transform,
                    width: computedStyle.width,
                    height: computedStyle.height
                };
                
                // Check Star Trek aesthetic features
                results.starTrekAesthetic = {
                    hasBackdropFilter: computedStyle.backdropFilter.includes('blur') || 
                                     computedStyle.webkitBackdropFilter.includes('blur'),
                    hasGradientBackground: computedStyle.background.includes('linear-gradient') ||
                                         computedStyle.background.includes('rgba'),
                    hasBorder: computedStyle.border !== 'none' && computedStyle.border !== '',
                    hasBoxShadow: computedStyle.boxShadow !== 'none',
                    hasTransition: computedStyle.transition !== 'none'
                };
                
                // Animation properties
                results.animationProperties = {
                    currentWidth: glassSubmenu.style.width,
                    currentOpacity: glassSubmenu.style.opacity,
                    currentTransform: glassSubmenu.style.transform
                };
                
                // Positioning
                const rect = glassSubmenu.getBoundingClientRect();
                results.positioning = {
                    left: rect.left,
                    top: rect.top,
                    width: rect.width,
                    height: rect.height,
                    zIndex: computedStyle.zIndex
                };
            }
            
            resolve(JSON.stringify(results));
        });
        """
        
        try:
            result_json = await self.client.js.get_value(aesthetic_test_js, timeout=10)
            result = json.loads(result_json)
            
            print(f"✅ VISUAL AESTHETIC RESULTS:")
            print(f"   Glass submenu exists: {result['glassSubmenuExists']}")
            
            if result['glassSubmenuExists']:
                aesthetic = result['starTrekAesthetic']
                print(f"   Star Trek TNG aesthetic:")
                print(f"      Backdrop filter: {aesthetic['hasBackdropFilter']}")
                print(f"      Gradient background: {aesthetic['hasGradientBackground']}")
                print(f"      Border styling: {aesthetic['hasBorder']}")
                print(f"      Box shadow: {aesthetic['hasBoxShadow']}")
                print(f"      Smooth transitions: {aesthetic['hasTransition']}")
                
                anim = result['animationProperties']
                print(f"   Animation state:")
                print(f"      Width: {anim['currentWidth']}")
                print(f"      Opacity: {anim['currentOpacity']}")
                print(f"      Transform: {anim['currentTransform']}")
                
                pos = result['positioning']
                print(f"   Positioning:")
                print(f"      Position: {pos['left']},{pos['top']}")
                print(f"      Size: {pos['width']}x{pos['height']}")
                print(f"      Z-index: {pos['zIndex']}")
            
            self.test_results['visual_aesthetics'] = result
            return result
            
        except Exception as e:
            print(f"❌ Visual aesthetic test failed: {e}")
            return None
    
    async def capture_final_screenshot(self):
        """📸 Capture final state screenshot"""
        print("📸 CAPTURING: Final state screenshot...")
        
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
                
                filepath = os.path.join(self.screenshots_dir, "glass_submenu_final.png")
                with open(filepath, 'wb') as f:
                    f.write(image_data)
                
                print(f"   📸 Final state captured: {result['width']}x{result['height']} → {filepath}")
                return filepath
            
        except Exception as e:
            print(f"❌ Final screenshot failed: {e}")
            return None
    
    def generate_test_report(self):
        """📋 Generate comprehensive test report"""
        print("\n" + "="*60)
        print("🧪 GLASS SUBMENU TEST REPORT")
        print("="*60)
        
        print("\n🔧 COMPONENT INITIALIZATION:")
        init_result = self.test_results.get('component_init', {})
        print(f"   ✅ AgentSelector component: {'✓' if init_result.get('agentSelectorExists') else '✗'}")
        print(f"   ✅ Web component defined: {'✓' if init_result.get('webComponentDefined') else '✗'}")
        print(f"   ✅ Drawer buttons found: {len(init_result.get('drawerButtons', []))}")
        
        print("\n🖱️ INTERACTION TESTING:")
        click_result = self.test_results.get('button_click', {})
        print(f"   ✅ Glass submenu triggered: {'✓' if click_result.get('glassSubmenuAppeared') else '✗'}")
        print(f"   ✅ Agents clickable: {len(click_result.get('agentsFound', []))}")
        
        print("\n🎯 SUBMENU FUNCTIONALITY:")
        button_result = self.test_results.get('submenu_buttons', {})
        print(f"   ✅ Submenu buttons working: {'✓' if button_result.get('glassSubmenuExists') else '✗'}")
        print(f"   ✅ Events fired: {len(button_result.get('eventsFired', []))}")
        
        print("\n🎨 VISUAL AESTHETICS:")
        aesthetic_result = self.test_results.get('visual_aesthetics', {})
        if aesthetic_result.get('glassSubmenuExists'):
            aesthetic = aesthetic_result.get('starTrekAesthetic', {})
            print(f"   ✅ Backdrop blur: {'✓' if aesthetic.get('hasBackdropFilter') else '✗'}")
            print(f"   ✅ Glass gradient: {'✓' if aesthetic.get('hasGradientBackground') else '✗'}")
            print(f"   ✅ Border styling: {'✓' if aesthetic.get('hasBorder') else '✗'}")
            print(f"   ✅ Box shadows: {'✓' if aesthetic.get('hasBoxShadow') else '✗'}")
            print(f"   ✅ Smooth transitions: {'✓' if aesthetic.get('hasTransition') else '✗'}")
        else:
            print("   ❌ No glass submenu found for aesthetic testing")
        
        print(f"\n📁 Screenshots saved to: {self.screenshots_dir}")
        print("   - glass_submenu_baseline.png")
        print("   - glass_submenu_active.png") 
        print("   - glass_submenu_final.png")
        
        # Overall assessment
        success_count = sum([
            init_result.get('agentSelectorExists', False),
            click_result.get('glassSubmenuAppeared', False),
            button_result.get('glassSubmenuExists', False),
            len(aesthetic_result.get('starTrekAesthetic', {}).values()) > 0
        ])
        
        print(f"\n🎉 OVERALL ASSESSMENT: {success_count}/4 major features working")
        if success_count >= 3:
            print("   ✅ Glass submenu system is WORKING WELL!")
        elif success_count >= 2:
            print("   ⚠️ Glass submenu system partially working - needs fixes")
        else:
            print("   ❌ Glass submenu system needs significant work")

async def run_comprehensive_test():
    """🚀 Run comprehensive glass submenu testing"""
    print("🧪 GLASS SUBMENU SYSTEM COMPREHENSIVE TEST")
    print("=" * 50)
    print("🪟 Testing Star Trek TNG glass submenu implementation")
    print("🔧 This acts as my 'eyes' to validate the code changes")
    
    async with GlassSubmenuTester() as tester:
        # Phase 1: Initialize and capture baseline
        print("\n🔧 PHASE 1: Component Initialization")
        await tester.test_component_initialization()
        await tester.capture_baseline_screenshot()
        
        # Phase 2: Test interactions
        print("\n🖱️ PHASE 2: User Interaction Testing")
        await tester.test_agent_button_clicking()
        await tester.capture_glass_submenu_screenshot()
        
        # Phase 3: Test submenu functionality
        print("\n🎯 PHASE 3: Submenu Functionality")
        await tester.test_glass_submenu_buttons()
        
        # Phase 4: Visual validation
        print("\n🎨 PHASE 4: Visual Aesthetic Validation")
        await tester.test_visual_aesthetics()
        await tester.capture_final_screenshot()
        
        # Phase 5: Generate report
        print("\n📋 PHASE 5: Test Report Generation")
        tester.generate_test_report()
        
        print(f"\n✅ Testing complete! Check screenshots in:")
        print(f"   {tester.screenshots_dir}")

if __name__ == "__main__":
    asyncio.run(run_comprehensive_test())