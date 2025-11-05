#!/usr/bin/env python3
"""
Component Issues Diagnosis
🔍 Diagnose why the AgentSelector component isn't appearing

This script helps me debug:
- DOM structure and element visibility
- Script loading and web component registration
- CSS and styling issues
- JavaScript errors and console output
"""

import asyncio
import json
import os
import sys
sys.path.append('/Users/joel/Development/ideem/vHSM/externals/continuum/python-client')

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class ComponentDiagnoser:
    """Diagnose component issues in detail"""
    
    def __init__(self):
        load_continuum_config()
        self.client = None
        self.screenshots_dir = "/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/screenshots"
        os.makedirs(self.screenshots_dir, exist_ok=True)
    
    async def __aenter__(self):
        self.client = ContinuumClient()
        await self.client.__aenter__()
        await self.client.register_agent({
            'agentId': 'component-diagnoser',
            'agentName': 'Component Diagnoser',
            'agentType': 'ai'
        })
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.client:
            await self.client.__aexit__(exc_type, exc_val, exc_tb)
    
    async def diagnose_dom_structure(self):
        """🔍 Deep dive into DOM structure"""
        print("🔍 DIAGNOSING: DOM structure...")
        
        dom_diagnosis_js = """
        return new Promise((resolve) => {
            const diagnosis = {
                pageTitle: document.title,
                agentSelectorElement: null,
                agentSelectorInDOM: false,
                mainAgentSelector: null,
                agentElements: [],
                webComponentSupport: {
                    customElementsSupported: typeof customElements !== 'undefined',
                    agentSelectorDefined: !!customElements.get('agent-selector'),
                    definedComponents: []
                },
                scriptElements: [],
                cssSelectors: {
                    agentSelector: !!document.querySelector('.agent-selector'),
                    agentList: !!document.querySelector('.agent-list'),
                    agentItem: !!document.querySelector('.agent-item')
                },
                sidebarStructure: {},
                console_logs: []
            };
            
            // Check if main-agent-selector exists
            const mainSelector = document.getElementById('main-agent-selector');
            diagnosis.mainAgentSelector = {
                exists: !!mainSelector,
                tagName: mainSelector?.tagName,
                className: mainSelector?.className,
                innerHTML: mainSelector?.innerHTML?.substring(0, 200),
                shadowRoot: !!mainSelector?.shadowRoot,
                connectedCallback: typeof mainSelector?.connectedCallback === 'function'
            };
            
            // Get all defined custom elements
            if (typeof customElements !== 'undefined') {
                // Can't directly iterate customElements, but check known ones
                const knownComponents = ['agent-selector', 'component-loader'];
                diagnosis.webComponentSupport.definedComponents = knownComponents.filter(name => 
                    customElements.get(name)
                );
            }
            
            // Find all agent-related elements
            const agentItems = Array.from(document.querySelectorAll('.agent-item, [data-agent-id]'));
            diagnosis.agentElements = agentItems.map(el => ({
                tagName: el.tagName,
                className: el.className,
                id: el.id,
                dataAgentId: el.dataset.agentId,
                text: el.textContent.trim().substring(0, 50),
                visible: el.offsetWidth > 0 && el.offsetHeight > 0,
                position: {
                    left: el.getBoundingClientRect().left,
                    top: el.getBoundingClientRect().top,
                    width: el.offsetWidth,
                    height: el.offsetHeight
                }
            }));
            
            // Check sidebar structure
            const sidebar = document.querySelector('.sidebar');
            if (sidebar) {
                diagnosis.sidebarStructure = {
                    exists: true,
                    children: Array.from(sidebar.children).map(child => ({
                        tagName: child.tagName,
                        className: child.className,
                        id: child.id
                    })),
                    agentSelectorInSidebar: !!sidebar.querySelector('#main-agent-selector'),
                    legacyAgentSelector: !!sidebar.querySelector('.agent-selector')
                };
            }
            
            // Get loaded scripts
            const scripts = Array.from(document.querySelectorAll('script[src]'));
            diagnosis.scriptElements = scripts.map(script => ({
                src: script.src,
                loaded: script.readyState === 'complete' || !script.readyState
            }));
            
            resolve(JSON.stringify(diagnosis, null, 2));
        });
        """
        
        try:
            result_json = await self.client.js.get_value(dom_diagnosis_js, timeout=15)
            result = json.loads(result_json)
            
            print("📋 DOM STRUCTURE DIAGNOSIS:")
            print(f"   Page title: {result['pageTitle']}")
            
            main_selector = result['mainAgentSelector']
            print(f"\n🎯 MAIN AGENT SELECTOR:")
            print(f"   Exists: {main_selector['exists']}")
            print(f"   Tag name: {main_selector['tagName']}")
            print(f"   Has shadow root: {main_selector['shadowRoot']}")
            print(f"   Connected callback: {main_selector['connectedCallback']}")
            
            web_support = result['webComponentSupport']
            print(f"\n🌐 WEB COMPONENT SUPPORT:")
            print(f"   Custom elements supported: {web_support['customElementsSupported']}")
            print(f"   AgentSelector defined: {web_support['agentSelectorDefined']}")
            print(f"   Defined components: {web_support['definedComponents']}")
            
            print(f"\n📂 SIDEBAR STRUCTURE:")
            sidebar = result['sidebarStructure']
            if sidebar.get('exists'):
                print(f"   AgentSelector in sidebar: {sidebar['agentSelectorInSidebar']}")
                print(f"   Legacy selector exists: {sidebar['legacyAgentSelector']}")
                print(f"   Sidebar children: {len(sidebar['children'])}")
                for child in sidebar['children'][:5]:  # Show first 5
                    print(f"      - {child['tagName']}.{child['className']} #{child['id']}")
            
            print(f"\n🎭 AGENT ELEMENTS FOUND: {len(result['agentElements'])}")
            for agent in result['agentElements'][:3]:  # Show first 3
                print(f"   - {agent['tagName']}: '{agent['text']}' (Visible: {agent['visible']})")
            
            print(f"\n📜 SCRIPTS LOADED: {len(result['scriptElements'])}")
            for script in result['scriptElements']:
                if 'AgentSelector' in script['src'] or 'component' in script['src'].lower():
                    print(f"   - {script['src']} (Loaded: {script['loaded']})")
            
            return result
            
        except Exception as e:
            print(f"❌ DOM diagnosis failed: {e}")
            return None
    
    async def check_script_loading(self):
        """📜 Check if scripts are loading correctly"""
        print("\n📜 CHECKING: Script loading status...")
        
        script_check_js = """
        return new Promise((resolve) => {
            const results = {
                agentSelectorScript: null,
                scriptErrors: [],
                networkErrors: [],
                consoleErrors: [],
                loadingStatus: {}
            };
            
            // Check for AgentSelector script specifically
            const agentSelectorScript = document.querySelector('script[src*="AgentSelector"]');
            if (agentSelectorScript) {
                results.agentSelectorScript = {
                    src: agentSelectorScript.src,
                    readyState: agentSelectorScript.readyState,
                    loaded: agentSelectorScript.readyState === 'complete'
                };
            }
            
            // Try to manually check if AgentSelector class exists
            results.loadingStatus = {
                windowAgentSelector: typeof window.AgentSelector !== 'undefined',
                customElementDefined: !!customElements.get('agent-selector'),
                globalAgentSelector: 'AgentSelector' in window
            };
            
            // Capture console errors
            const originalError = console.error;
            const errors = [];
            console.error = function(...args) {
                errors.push(args.join(' '));
                originalError.apply(console, args);
            };
            results.consoleErrors = errors;
            
            resolve(JSON.stringify(results));
        });
        """
        
        try:
            result_json = await self.client.js.get_value(script_check_js, timeout=10)
            result = json.loads(result_json)
            
            print("📜 SCRIPT LOADING RESULTS:")
            script_info = result.get('agentSelectorScript')
            if script_info:
                print(f"   AgentSelector script found: {script_info['src']}")
                print(f"   Ready state: {script_info['readyState']}")
                print(f"   Loaded: {script_info['loaded']}")
            else:
                print("   ❌ AgentSelector script NOT found in DOM")
            
            status = result['loadingStatus']
            print(f"\n🔍 LOADING STATUS:")
            print(f"   Window.AgentSelector: {status['windowAgentSelector']}")
            print(f"   Custom element defined: {status['customElementDefined']}")
            print(f"   Global AgentSelector: {status['globalAgentSelector']}")
            
            if result['consoleErrors']:
                print(f"\n⚠️ CONSOLE ERRORS: {len(result['consoleErrors'])}")
                for error in result['consoleErrors'][:3]:
                    print(f"   - {error}")
            
            return result
            
        except Exception as e:
            print(f"❌ Script check failed: {e}")
            return None
    
    async def try_manual_component_creation(self):
        """🔧 Try to manually create the component"""
        print("\n🔧 TRYING: Manual component creation...")
        
        manual_creation_js = """
        return new Promise((resolve) => {
            const results = {
                manualCreationAttempt: false,
                agentSelectorCreated: false,
                componentAppended: false,
                shadowRootCreated: false,
                renderCalled: false,
                finalState: {},
                errors: []
            };
            
            try {
                // Try to manually create an agent-selector element
                const agentSelector = document.createElement('agent-selector');
                results.agentSelectorCreated = true;
                
                // Try to append it to the sidebar
                const sidebar = document.querySelector('.sidebar');
                if (sidebar) {
                    // Remove any existing main-agent-selector first
                    const existing = document.getElementById('main-agent-selector');
                    if (existing) {
                        existing.remove();
                    }
                    
                    agentSelector.id = 'main-agent-selector-manual';
                    sidebar.appendChild(agentSelector);
                    results.componentAppended = true;
                    
                    // Wait for connection
                    setTimeout(() => {
                        results.shadowRootCreated = !!agentSelector.shadowRoot;
                        results.finalState = {
                            tagName: agentSelector.tagName,
                            id: agentSelector.id,
                            className: agentSelector.className,
                            shadowRoot: !!agentSelector.shadowRoot,
                            innerHTML: agentSelector.innerHTML,
                            visible: agentSelector.offsetWidth > 0 && agentSelector.offsetHeight > 0
                        };
                        resolve(JSON.stringify(results));
                    }, 500);
                } else {
                    results.errors.push('Sidebar not found');
                    resolve(JSON.stringify(results));
                }
                
                results.manualCreationAttempt = true;
                
            } catch (error) {
                results.errors.push(error.message);
                resolve(JSON.stringify(results));
            }
        });
        """
        
        try:
            result_json = await self.client.js.get_value(manual_creation_js, timeout=10)
            result = json.loads(result_json)
            
            print("🔧 MANUAL CREATION RESULTS:")
            print(f"   Creation attempted: {result['manualCreationAttempt']}")
            print(f"   AgentSelector created: {result['agentSelectorCreated']}")
            print(f"   Component appended: {result['componentAppended']}")
            print(f"   Shadow root created: {result['shadowRootCreated']}")
            
            final_state = result.get('finalState', {})
            if final_state:
                print(f"   Final component state:")
                print(f"      Tag: {final_state['tagName']}")
                print(f"      ID: {final_state['id']}")
                print(f"      Shadow root: {final_state['shadowRoot']}")
                print(f"      Visible: {final_state['visible']}")
            
            if result['errors']:
                print(f"   ❌ Errors: {result['errors']}")
            
            return result
            
        except Exception as e:
            print(f"❌ Manual creation failed: {e}")
            return None
    
    async def capture_diagnosis_screenshot(self):
        """📸 Capture screenshot of current state"""
        print("\n📸 CAPTURING: Diagnosis screenshot...")
        
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
                
                filepath = os.path.join(self.screenshots_dir, "component_diagnosis.png")
                with open(filepath, 'wb') as f:
                    f.write(image_data)
                
                print(f"   📸 Diagnosis captured: {result['width']}x{result['height']} → {filepath}")
                return filepath
            
        except Exception as e:
            print(f"❌ Diagnosis screenshot failed: {e}")
            return None

async def run_component_diagnosis():
    """🔍 Run comprehensive component diagnosis"""
    print("🔍 COMPONENT ISSUES DIAGNOSIS")
    print("=" * 40)
    print("🪟 Diagnosing why AgentSelector component isn't appearing")
    
    async with ComponentDiagnoser() as diagnoser:
        # Phase 1: DOM structure analysis
        await diagnoser.diagnose_dom_structure()
        
        # Phase 2: Script loading check
        await diagnoser.check_script_loading()
        
        # Phase 3: Manual creation attempt
        await diagnoser.try_manual_component_creation()
        
        # Phase 4: Visual evidence
        await diagnoser.capture_diagnosis_screenshot()
        
        print(f"\n✅ Diagnosis complete! Check screenshot:")
        print(f"   {diagnoser.screenshots_dir}/component_diagnosis.png")

if __name__ == "__main__":
    asyncio.run(run_component_diagnosis())