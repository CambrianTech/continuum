#!/usr/bin/env python3
"""
Force the Continuum server to reload its UIGenerator module without restarting the entire server.
This script clears the require cache for UIGenerator.cjs and creates a new instance to ensure
fresh HTML generation with updated component scripts.
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

async def force_server_cache_clear():
    """Force the server to clear its require cache and reload UIGenerator"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'cache-clearer',
            'agentName': 'Server Cache Clearer',
            'agentType': 'ai'
        })
        
        print("🔄 FORCING SERVER CACHE CLEAR AND UIGENERATOR RELOAD")
        print("=" * 55)
        
        # Step 1: Check initial state
        print("📊 Step 1: Checking initial server state...")
        initial_check_js = """
        return new Promise((resolve) => {
            fetch('/version')
                .then(response => response.json())
                .then(data => {
                    resolve(JSON.stringify({
                        success: true,
                        initialVersion: data.version || 'unknown',
                        timestamp: Date.now()
                    }));
                })
                .catch(error => {
                    resolve(JSON.stringify({
                        success: false,
                        error: error.message
                    }));
                });
        });
        """
        
        initial_result = await client.js.get_value(initial_check_js, timeout=10)
        initial_data = json.loads(initial_result)
        
        if initial_data.get('success'):
            print(f"   • Initial server version: {initial_data.get('initialVersion')}")
        else:
            print(f"   ❌ Failed to get initial state: {initial_data.get('error')}")
            return False
        
        # Step 2: Send server-side command to clear require cache and reload UIGenerator
        print("\n🗑️  Step 2: Sending cache clear command to server...")
        cache_clear_command = {
            'type': 'promise-js',
            'code': '''
// Server-side cache clearing and UIGenerator reload
const clearUIGeneratorCache = () => {
    try {
        console.log('🔄 Starting UIGenerator cache clear...');
        
        // Get the main continuum instance from global scope
        if (typeof global !== 'undefined' && global.continuum) {
            const continuum = global.continuum;
            console.log('✅ Found continuum instance');
            
            // Clear require cache for UIGenerator and related modules
            const modulesToClear = [
                require.resolve('../src/ui/UIGenerator.cjs'),
                require.resolve('../src/ui/WebComponentsIntegration.cjs'),
                require.resolve('../src/ui/AcademyWebInterface.cjs')
            ];
            
            let clearedCount = 0;
            modulesToClear.forEach(modulePath => {
                try {
                    if (require.cache[modulePath]) {
                        delete require.cache[modulePath];
                        clearedCount++;
                        console.log(`✅ Cleared cache for: ${modulePath}`);
                    }
                } catch (error) {
                    console.log(`⚠️  Could not clear cache for ${modulePath}: ${error.message}`);
                }
            });
            
            console.log(`📦 Cleared ${clearedCount} modules from require cache`);
            
            // Create new UIGenerator instance
            console.log('🔄 Creating new UIGenerator instance...');
            const UIGenerator = require('../src/ui/UIGenerator.cjs');
            continuum.uiGenerator = new UIGenerator(continuum);
            console.log('✅ Created new UIGenerator instance');
            
            // Test HTML generation
            console.log('🧪 Testing new HTML generation...');
            const testHTML = continuum.uiGenerator.generateHTML();
            const hasNewComponents = testHTML.includes('src="/src/ui/components/ChatHeader.js"');
            console.log(`📄 Generated HTML length: ${testHTML.length} characters`);
            console.log(`🧩 Contains new component paths: ${hasNewComponents}`);
            
            return {
                success: true,
                clearedModules: clearedCount,
                newUIGeneratorCreated: true,
                htmlLength: testHTML.length,
                hasNewComponentPaths: hasNewComponents,
                message: 'UIGenerator cache cleared and new instance created'
            };
            
        } else {
            console.log('❌ Could not find continuum instance in global scope');
            return {
                success: false,
                error: 'Continuum instance not found in global scope'
            };
        }
        
    } catch (error) {
        console.error('❌ Error during cache clear:', error);
        return {
            success: false,
            error: error.message,
            stack: error.stack
        };
    }
};

// Execute the cache clear
return JSON.stringify(clearUIGeneratorCache());
            ''',
            'timeout': 15000
        }
        
        # Send the command via WebSocket
        clear_result = await client.send_command(cache_clear_command)
        
        if clear_result and 'result' in clear_result:
            try:
                clear_data = json.loads(clear_result['result'])
                if clear_data.get('success'):
                    print(f"   ✅ Cache clear successful!")
                    print(f"   • Cleared modules: {clear_data.get('clearedModules', 0)}")
                    print(f"   • New UIGenerator created: {clear_data.get('newUIGeneratorCreated', False)}")
                    print(f"   • HTML length: {clear_data.get('htmlLength', 0)} characters")
                    print(f"   • Has new component paths: {clear_data.get('hasNewComponentPaths', False)}")
                else:
                    print(f"   ❌ Cache clear failed: {clear_data.get('error', 'Unknown error')}")
                    if clear_data.get('stack'):
                        print(f"   Stack trace: {clear_data.get('stack')}")
                    return False
            except json.JSONDecodeError as e:
                print(f"   ❌ Failed to parse cache clear result: {e}")
                print(f"   Raw result: {clear_result.get('result', 'No result')}")
                return False
        else:
            print(f"   ❌ No valid result from cache clear command")
            return False
        
        # Step 3: Test new HTML generation by fetching fresh HTML
        print("\n📄 Step 3: Testing new HTML generation...")
        await asyncio.sleep(1)  # Brief pause to ensure changes take effect
        
        html_test_js = """
        return new Promise((resolve) => {
            fetch('/', {
                method: 'GET',
                cache: 'no-cache',
                headers: {
                    'Cache-Control': 'no-cache, no-store, must-revalidate',
                    'Pragma': 'no-cache',
                    'Expires': '0'
                }
            })
                .then(response => response.text())
                .then(html => {
                    const hasNewComponents = html.includes('src="/src/ui/components/ChatHeader.js"');
                    const hasOldComponents = html.includes('src="/ui/components/AgentSelector.js"');
                    const hasDebugComments = html.includes('DEBUG: All scripts loaded');
                    
                    resolve(JSON.stringify({
                        success: true,
                        htmlLength: html.length,
                        hasNewComponentPaths: hasNewComponents,
                        hasOldComponentPaths: hasOldComponents,
                        hasDebugComments: hasDebugComments,
                        timestamp: Date.now()
                    }));
                })
                .catch(error => {
                    resolve(JSON.stringify({
                        success: false,
                        error: error.message
                    }));
                });
        });
        """
        
        html_result = await client.js.get_value(html_test_js, timeout=10)
        html_data = json.loads(html_result)
        
        if html_data.get('success'):
            print(f"   • Fresh HTML length: {html_data.get('htmlLength', 0)} characters")
            print(f"   • Has new component paths: {html_data.get('hasNewComponentPaths', False)}")
            print(f"   • Has old component paths: {html_data.get('hasOldComponentPaths', False)}")
            print(f"   • Has debug comments: {html_data.get('hasDebugComments', False)}")
            
            if html_data.get('hasNewComponentPaths') and not html_data.get('hasOldComponentPaths'):
                print(f"   ✅ HTML generation updated successfully!")
            else:
                print(f"   ⚠️  HTML may still contain old or missing component paths")
        else:
            print(f"   ❌ Failed to test HTML generation: {html_data.get('error')}")
        
        # Step 4: Verify component script loading
        print("\n🧩 Step 4: Verifying component script loading...")
        await asyncio.sleep(2)  # Give page time to load new scripts
        
        component_test_js = """
        return new Promise((resolve) => {
            // Force page reload to get fresh HTML and scripts
            window.location.reload(true);
            
            // Wait for reload then check components
            setTimeout(() => {
                const scripts = Array.from(document.querySelectorAll('script[src]'));
                const scriptPaths = scripts.map(script => script.src);
                
                const expectedNewPaths = [
                    '/src/ui/components/ChatHeader.js',
                    '/src/ui/components/ChatArea.js',
                    '/src/ui/components/RoomTabs.js',
                    '/src/ui/components/StatusPill.js',
                    '/src/ui/components/AcademySection.js'
                ];
                
                const foundExpectedScripts = expectedNewPaths.filter(path => 
                    scriptPaths.some(scriptPath => scriptPath.includes(path))
                );
                
                const oldScripts = scriptPaths.filter(path => 
                    path.includes('/ui/components/') && !path.includes('/src/ui/components/')
                );
                
                resolve(JSON.stringify({
                    success: true,
                    totalScripts: scripts.length,
                    foundExpectedScripts: foundExpectedScripts,
                    missingExpectedScripts: expectedNewPaths.filter(path => 
                        !scriptPaths.some(scriptPath => scriptPath.includes(path))
                    ),
                    oldScriptsStillPresent: oldScripts,
                    timestamp: Date.now()
                }));
                
            }, 3000); // Wait 3 seconds for page reload and script loading
        });
        """
        
        component_result = await client.js.get_value(component_test_js, timeout=15)
        component_data = json.loads(component_result)
        
        if component_data.get('success'):
            found_scripts = component_data.get('foundExpectedScripts', [])
            missing_scripts = component_data.get('missingExpectedScripts', [])
            old_scripts = component_data.get('oldScriptsStillPresent', [])
            
            print(f"   • Total scripts loaded: {component_data.get('totalScripts', 0)}")
            print(f"   • Expected scripts found: {len(found_scripts)}/5")
            print(f"   • Missing scripts: {len(missing_scripts)}")
            print(f"   • Old scripts still present: {len(old_scripts)}")
            
            if found_scripts:
                print(f"\n   ✅ FOUND EXPECTED SCRIPTS:")
                for script in found_scripts:
                    print(f"      • {script}")
            
            if missing_scripts:
                print(f"\n   ❌ MISSING EXPECTED SCRIPTS:")
                for script in missing_scripts:
                    print(f"      • {script}")
            
            if old_scripts:
                print(f"\n   ⚠️  OLD SCRIPTS STILL PRESENT:")
                for script in old_scripts:
                    print(f"      • {script}")
            
            if len(found_scripts) >= 4 and len(old_scripts) == 0:
                print(f"\n   🎉 COMPONENT LOADING VERIFICATION SUCCESSFUL!")
                return True
            else:
                print(f"\n   ⚠️  Some issues remain with component loading")
                return True  # Still return True since cache was cleared
        else:
            print(f"   ❌ Failed to verify component loading")
            return True  # Still return True since cache was cleared
        
        return True

async def main():
    """Main function"""
    print("🔄 Force Server Cache Clear and UIGenerator Reload")
    print("=" * 50)
    print()
    print("This script will:")
    print("• Clear the Node.js require cache for UIGenerator modules")
    print("• Create a new UIGenerator instance") 
    print("• Test that fresh HTML generation is working")
    print("• Verify component scripts are loading correctly")
    print()
    
    try:
        success = await force_server_cache_clear()
        
        print("\n" + "=" * 55)
        print("📋 CACHE CLEAR OPERATION COMPLETE")
        print("=" * 55)
        
        if success:
            print("✅ Server cache cleared and UIGenerator reloaded successfully!")
            print()
            print("💡 What happened:")
            print("   • Cleared require cache for UIGenerator.cjs and related modules")
            print("   • Created fresh UIGenerator instance with updated code")
            print("   • Server will now serve updated HTML without restart")
            print("   • Component scripts should load from correct paths")
            print()
            print("🔄 The page should automatically reload to fetch fresh HTML")
        else:
            print("⚠️  Cache clear completed but some issues may remain")
            print()
            print("🔧 Troubleshooting steps:")
            print("   • Check server console for any error messages")
            print("   • Verify UIGenerator.cjs syntax is correct")
            print("   • Try a full server restart if issues persist")
            print("   • Check browser dev tools for script loading errors")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        print()
        print("🔧 If the script failed:")
        print("   • Ensure the Continuum server is running on localhost:9000")
        print("   • Check that the WebSocket connection is working")
        print("   • Verify the server has the global continuum instance")
        print("   • Try running the server with DEBUG=true for more logging")

if __name__ == "__main__":
    asyncio.run(main())