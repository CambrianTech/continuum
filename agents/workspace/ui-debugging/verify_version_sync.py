#!/usr/bin/env python3
"""
Verify version sync between client and server
Ensures browser client is using the latest server version
"""
import asyncio
import json
import sys
import urllib.request
import urllib.error
from pathlib import Path

# Add python-client to path
script_dir = Path(__file__).parent
client_dir = script_dir.parent.parent.parent / 'python-client'
sys.path.append(str(client_dir))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def verify_version_sync():
    """Verify client and server versions are in sync"""
    load_continuum_config()
    
    # First check server version via HTTP
    try:
        with urllib.request.urlopen('http://localhost:9000/version') as response:
            server_version_data = json.loads(response.read().decode())
        server_version = server_version_data.get('version', 'unknown')
        print(f"🖥️  Server Version: {server_version}")
    except Exception as e:
        print(f"❌ Failed to get server version: {e}")
        return False
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'version-sync-checker',
            'agentName': 'Version Sync Checker',
            'agentType': 'ai'
        })
        
        print("🔍 CHECKING VERSION SYNC")
        print("=" * 30)
        
        # Check client-side version
        version_check_js = """
        return new Promise((resolve) => {
            console.log('🔍 Checking client version...');
            
            // Look for version in various places
            let clientVersion = 'unknown';
            let versionSource = 'not found';
            
            // Check currentVersion JavaScript variable (primary location)
            if (typeof currentVersion !== 'undefined') {
                clientVersion = currentVersion;
                versionSource = 'currentVersion variable';
            }
            
            // Check if version is in window object
            if (typeof window.ContinuumVersion !== 'undefined') {
                clientVersion = window.ContinuumVersion;
                versionSource = 'window.ContinuumVersion';
            }
            
            // Check version badge in DOM
            const versionBadge = document.querySelector('.version-badge');
            if (versionBadge && versionBadge.textContent) {
                const badgeMatch = versionBadge.textContent.match(/v?(\\d+\\.\\d+\\.\\d+)/);
                if (badgeMatch) {
                    clientVersion = badgeMatch[1];
                    versionSource = 'version badge';
                }
            }
            
            // Check if version is in HTML meta tags
            const versionMeta = document.querySelector('meta[name="version"]');
            if (versionMeta) {
                clientVersion = versionMeta.content;
                versionSource = 'meta tag';
            }
            
            // Check if version is in body data attribute
            const bodyVersion = document.body.getAttribute('data-version');
            if (bodyVersion) {
                clientVersion = bodyVersion;
                versionSource = 'body data-version';
            }
            
            // Get cache info
            const cacheInfo = {
                localStorage: typeof localStorage !== 'undefined' ? localStorage.length : 0,
                sessionStorage: typeof sessionStorage !== 'undefined' ? sessionStorage.length : 0,
                lastReload: sessionStorage.getItem('lastReload') || 'never'
            };
            
            resolve(JSON.stringify({
                success: true,
                clientVersion: clientVersion,
                versionSource: versionSource,
                cache: cacheInfo,
                timestamp: Date.now()
            }));
        });
        """
        
        print("🔍 Checking client version...")
        result = await client.js.get_value(version_check_js, timeout=10)
        data = json.loads(result)
        
        if data.get('success'):
            client_version = data.get('clientVersion', 'unknown')
            version_source = data.get('versionSource', 'unknown')
            cache_info = data.get('cache', {})
            
            print(f"🌐 Client Version: {client_version} (from {version_source})")
            print(f"📊 Cache Info: localStorage={cache_info.get('localStorage', 0)}, sessionStorage={cache_info.get('sessionStorage', 0)}")
            print(f"⏰ Last Reload: {cache_info.get('lastReload', 'never')}")
            
            # Check if versions match
            versions_match = server_version == client_version
            print(f"\n🔄 Version Sync: {'✅ MATCH' if versions_match else '❌ MISMATCH'}")
            
            if not versions_match:
                print(f"⚠️  Version Mismatch Detected!")
                print(f"   Server: {server_version}")
                print(f"   Client: {client_version}")
                print(f"\n🔄 Forcing hard refresh with cache clear...")
                
                # Force hard refresh with cache clearing
                force_refresh_js = """
                return new Promise((resolve) => {
                    console.log('🔄 Forcing hard refresh with cache clear...');
                    
                    // Clear all possible caches
                    if (typeof localStorage !== 'undefined') {
                        localStorage.clear();
                    }
                    if (typeof sessionStorage !== 'undefined') {
                        sessionStorage.clear();
                    }
                    
                    // Mark reload time
                    sessionStorage.setItem('lastReload', Date.now().toString());
                    
                    // Force hard refresh (bypasses cache)
                    window.location.reload(true);
                    
                    resolve(JSON.stringify({
                        success: true,
                        action: 'hard_refresh_initiated'
                    }));
                });
                """
                
                await client.js.get_value(force_refresh_js, timeout=5)
                print("✅ Hard refresh initiated")
                
                # Wait for page to reload
                await asyncio.sleep(3)
                
                # Re-check version after refresh
                print("\n🔍 Re-checking version after refresh...")
                post_refresh_result = await client.js.get_value(version_check_js, timeout=10)
                post_refresh_data = json.loads(post_refresh_result)
                
                if post_refresh_data.get('success'):
                    new_client_version = post_refresh_data.get('clientVersion', 'unknown')
                    print(f"🌐 Client Version After Refresh: {new_client_version}")
                    final_match = server_version == new_client_version
                    print(f"🔄 Final Version Sync: {'✅ MATCH' if final_match else '❌ STILL MISMATCH'}")
                    
                    if final_match:
                        print("✅ Version sync successful!")
                        return True
                    else:
                        print("❌ Version sync failed - manual intervention needed")
                        return False
            else:
                print("✅ Versions already in sync!")
                return True
        
        return False

async def main():
    """Main function"""
    print("🔍 Version Sync Checker")
    print("=" * 25)
    print()
    
    try:
        sync_success = await verify_version_sync()
        
        if sync_success:
            print("\n" + "=" * 30)
            print("✅ VERSION SYNC COMPLETE")
            print("=" * 30)
            print()
            print("💡 Next Steps:")
            print("   • Test component loading again")
            print("   • Verify UI elements appear correctly")
            print("   • Check for JavaScript errors")
        else:
            print("\n" + "=" * 30)
            print("❌ VERSION SYNC FAILED")
            print("=" * 30)
            print()
            print("🔧 Manual Steps Needed:")
            print("   • Check server is running latest version")
            print("   • Manually clear browser cache")
            print("   • Use incognito/private browsing mode")
            print("   • Restart Continuum server if needed")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        print("🔧 Troubleshooting:")
        print("   • Ensure Continuum server is running")
        print("   • Check WebSocket connection")
        print("   • Verify browser has active Continuum tab")

if __name__ == "__main__":
    asyncio.run(main())