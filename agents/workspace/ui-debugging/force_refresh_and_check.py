#!/usr/bin/env python3
"""
Force refresh the page and check if components load correctly
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

async def force_refresh_and_check():
    """Force refresh and check component loading"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'refresh-checker',
            'agentName': 'Refresh and Check Components',
            'agentType': 'ai'
        })
        
        print("🔄 FORCING PAGE REFRESH AND CHECKING COMPONENTS")
        print("=" * 50)
        
        # Force hard refresh
        refresh_js = """
        return new Promise((resolve) => {
            console.log('🔄 Forcing hard refresh to clear cache...');
            
            // Force reload with cache bypass
            window.location.reload(true);
            
            resolve(JSON.stringify({
                success: true,
                message: 'Refresh initiated'
            }));
        });
        """
        
        print("🔄 Forcing hard refresh to clear cache...")
        await client.js.get_value(refresh_js, timeout=5)
        
        # Wait for page to reload
        print("⏳ Waiting for page to reload...")
        await asyncio.sleep(5)
        
        # Check script loading after refresh
        check_after_refresh_js = """
        return new Promise((resolve) => {
            console.log('🔍 Checking components after refresh...');
            
            // Wait a moment for scripts to load
            setTimeout(() => {
                const scripts = Array.from(document.querySelectorAll('script[src]'));
                const scriptPaths = scripts.map(script => script.src);
                
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
                
                // Check for the specific scripts we expect
                const expectedPaths = [
                    '/src/ui/components/ChatHeader.js',
                    '/src/ui/components/ChatArea.js',
                    '/src/ui/components/RoomTabs.js',
                    '/src/ui/components/StatusPill.js',
                    '/src/ui/components/AcademySection.js'
                ];
                
                const foundExpectedScripts = expectedPaths.filter(path => 
                    scriptPaths.some(scriptPath => scriptPath.includes(path))
                );
                
                resolve(JSON.stringify({
                    success: true,
                    totalScripts: scripts.length,
                    scriptPaths: scriptPaths,
                    components: components,
                    foundExpectedScripts: foundExpectedScripts,
                    missingExpectedScripts: expectedPaths.filter(path => 
                        !scriptPaths.some(scriptPath => scriptPath.includes(path))
                    )
                }));
            }, 2000); // Wait 2 seconds for scripts to load
        });
        """
        
        print("🔍 Checking components after refresh...")
        result = await client.js.get_value(check_after_refresh_js, timeout=20)
        data = json.loads(result)
        
        if data.get('success'):
            script_paths = data.get('scriptPaths', [])
            components = data.get('components', [])
            found_scripts = data.get('foundExpectedScripts', [])
            missing_scripts = data.get('missingExpectedScripts', [])
            
            print(f"\n📊 RESULTS AFTER REFRESH:")
            print(f"   • Total scripts loaded: {data.get('totalScripts', 0)}")
            print(f"   • Expected scripts found: {len(found_scripts)}")
            print(f"   • Expected scripts missing: {len(missing_scripts)}")
            
            if found_scripts:
                print(f"\n✅ FOUND EXPECTED SCRIPTS:")
                for script in found_scripts:
                    print(f"   • {script}")
            
            if missing_scripts:
                print(f"\n❌ MISSING EXPECTED SCRIPTS:")
                for script in missing_scripts:
                    print(f"   • {script}")
            
            print(f"\n📋 ALL LOADED SCRIPTS:")
            for script_path in script_paths:
                print(f"   • {script_path}")
            
            print(f"\n🧩 COMPONENT STATUS:")
            all_working = True
            for component in components:
                exists_status = "✅" if component.get('exists', False) else "❌"
                defined_status = "✅" if component.get('defined', False) else "❌"
                working = component.get('exists', False) and component.get('defined', False)
                if not working:
                    all_working = False
                print(f"   {component.get('tagName', 'Unknown')}: {exists_status} DOM, {defined_status} Defined")
            
            if all_working:
                print(f"\n🎉 ALL COMPONENTS WORKING!")
            else:
                print(f"\n⚠️  Some components still not working")
        
        return True

async def main():
    """Main function"""
    print("🔄 Force Refresh and Component Check")
    print("=" * 35)
    print()
    
    try:
        await force_refresh_and_check()
        
        print("\n" + "=" * 50)
        print("📋 REFRESH CHECK COMPLETE")
        print("=" * 50)
        print()
        print("💡 If components are still missing:")
        print("   • Check browser dev tools Network tab for 404s")
        print("   • Verify UIGenerator is serving correct paths")
        print("   • Check HttpServer static file serving")
        print("   • Try manual component URLs in browser")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")

if __name__ == "__main__":
    asyncio.run(main())