#!/usr/bin/env python3
"""
Debug component loading issues
"""
import asyncio
import json
import sys
from pathlib import Path

# Add python-client to path
script_dir = Path(__file__).parent
client_dir = script_dir.parent.parent.parent / 'python-client'
sys.path.append(str(client_dir))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def debug_component_loading():
    """Debug why components aren't loading"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'component-debugger',
            'agentName': 'Component Loading Debugger',
            'agentType': 'ai'
        })
        
        print("🔧 DEBUGGING COMPONENT LOADING")
        print("=" * 40)
        
        # Check script loading
        script_check_js = """
        return new Promise((resolve) => {
            console.log('🔍 Checking script loading...');
            
            const scripts = Array.from(document.querySelectorAll('script[src]'));
            const scriptInfo = scripts.map(script => ({
                src: script.src,
                loaded: script.readyState === 'complete' || script.readyState === undefined,
                error: script.onerror !== null,
                type: script.type || 'text/javascript'
            }));
            
            // Check for expected component scripts
            const expectedScripts = [
                'AgentSelector.js',
                'ChatHeader.js', 
                'ChatArea.js',
                'RoomTabs.js',
                'StatusPill.js',
                'AcademySection.js'
            ];
            
            const missingScripts = expectedScripts.filter(scriptName => 
                !scripts.some(script => script.src.includes(scriptName))
            );
            
            // Check DOM for components
            const components = [
                'agent-selector',
                'chat-area', 
                'chat-header',
                'room-tabs',
                'status-pill',
                'academy-section'
            ].map(tagName => ({
                tagName,
                exists: document.querySelector(tagName) !== null,
                defined: customElements.get(tagName) !== undefined
            }));
            
            // Check for network errors
            const networkErrors = [];
            
            resolve(JSON.stringify({
                success: true,
                scripts: scriptInfo,
                missingScripts: missingScripts,
                components: components,
                networkErrors: networkErrors,
                totalScripts: scripts.length
            }));
        });
        """
        
        print("🔍 Checking script and component loading...")
        result = await client.js.get_value(script_check_js, timeout=15)
        data = json.loads(result)
        
        if data.get('success'):
            scripts = data.get('scripts', [])
            missing_scripts = data.get('missingScripts', [])
            components = data.get('components', [])
            
            print(f"\n📊 SCRIPT LOADING:")
            print(f"   • Total scripts: {data.get('totalScripts', 0)}")
            
            if missing_scripts:
                print(f"   ❌ Missing scripts:")
                for script in missing_scripts:
                    print(f"      - {script}")
            else:
                print(f"   ✅ All expected scripts found")
            
            print(f"\n📋 LOADED SCRIPTS:")
            for script in scripts:
                status = "✅" if script.get('loaded', False) else "❌"
                print(f"   {status} {script.get('src', 'Unknown')}")
                if not script.get('loaded', True):
                    print(f"      Status: {script.get('error', 'Loading...')}")
            
            print(f"\n🧩 COMPONENT STATUS:")
            for component in components:
                exists_status = "✅" if component.get('exists', False) else "❌"
                defined_status = "✅" if component.get('defined', False) else "❌"
                print(f"   {component.get('tagName', 'Unknown')}:")
                print(f"      DOM exists: {exists_status}")
                print(f"      Defined: {defined_status}")
        
        # Check console errors that might have occurred during loading
        console_errors_js = """
        return new Promise((resolve) => {
            console.log('🔍 Checking for loading errors...');
            
            // Temporarily override console.error to catch new errors
            const errors = [];
            const originalError = console.error;
            
            console.error = function(...args) {
                errors.push(args.join(' '));
                originalError.apply(console, args);
            };
            
            // Try to manually load a component to test loading
            let manualLoadTest = 'not attempted';
            try {
                // Don't actually create elements, just test if we can
                if (typeof customElements !== 'undefined') {
                    manualLoadTest = `customElements available, ${customElements.get('agent-selector') ? 'agent-selector defined' : 'agent-selector not defined'}`;
                }
            } catch (e) {
                manualLoadTest = `Error: ${e.message}`;
            }
            
            // Restore console.error
            console.error = originalError;
            
            resolve(JSON.stringify({
                success: true,
                newErrors: errors,
                manualLoadTest: manualLoadTest
            }));
        });
        """
        
        print(f"\n🔍 Checking for loading errors...")
        error_result = await client.js.get_value(console_errors_js, timeout=10)
        error_data = json.loads(error_result)
        
        if error_data.get('success'):
            new_errors = error_data.get('newErrors', [])
            manual_test = error_data.get('manualLoadTest', 'Unknown')
            
            print(f"   📊 Manual load test: {manual_test}")
            
            if new_errors:
                print(f"   ❌ New errors during check:")
                for error in new_errors:
                    print(f"      - {error}")
            else:
                print(f"   ✅ No new errors detected")
        
        return True

async def main():
    """Main debugging function"""
    print("🔧 Component Loading Debugger")
    print("=" * 30)
    print()
    
    try:
        await debug_component_loading()
        
        print("\n" + "=" * 40)
        print("📋 DEBUGGING COMPLETE")
        print("=" * 40)
        print()
        print("🔧 Likely Causes:")
        print("   • Component script files not found (404 errors)")
        print("   • Script loading order issues")
        print("   • Path resolution problems in UIGenerator")
        print("   • Component definition errors")
        print()
        print("💡 Next Steps:")
        print("   • Check browser Network tab for 404 errors")
        print("   • Verify component file paths in UIGenerator")
        print("   • Check if files exist in src/ui/components/")
        print("   • Test manual component loading")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")

if __name__ == "__main__":
    asyncio.run(main())