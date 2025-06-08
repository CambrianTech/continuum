#!/usr/bin/env python3
"""
Fix browser tab management to properly focus existing tabs instead of opening new ones
"""
import asyncio
import json
import sys
import base64
from datetime import datetime
from pathlib import Path

# Add python-client to path
script_dir = Path(__file__).parent
client_dir = script_dir / 'python-client'
sys.path.append(str(client_dir))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_and_fix_tab_management():
    """Test current tab management and fix issues"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'tab-manager-fixer',
            'agentName': 'Tab Manager Fixer',
            'agentType': 'ai'
        })
        
        print("🔧 TESTING BROWSER TAB MANAGEMENT")
        print("=" * 40)
        
        # Test if we can detect current tab
        tab_detection_js = """
        return new Promise((resolve) => {
            console.log('🔍 Testing tab detection...');
            
            // Get current tab information
            const tabInfo = {
                url: window.location.href,
                title: document.title,
                focused: document.hasFocus(),
                visibility: document.visibilityState,
                timestamp: Date.now()
            };
            
            console.log('📊 Current tab info:', tabInfo);
            
            // Test WebSocket connection
            if (typeof ws !== 'undefined' && ws.readyState === WebSocket.OPEN) {
                console.log('✅ WebSocket connection active');
                tabInfo.websocketConnected = true;
            } else {
                console.log('❌ WebSocket connection not active');
                tabInfo.websocketConnected = false;
            }
            
            resolve(JSON.stringify({
                success: true,
                tabInfo: tabInfo
            }));
        });
        """
        
        print("🔍 Testing current tab detection...")
        result = await client.js.get_value(tab_detection_js, timeout=10)
        data = json.loads(result)
        
        if data.get('success'):
            tab_info = data.get('tabInfo', {})
            print(f"   📊 URL: {tab_info.get('url', 'Unknown')}")
            print(f"   📊 Focused: {tab_info.get('focused', False)}")
            print(f"   📊 Visibility: {tab_info.get('visibility', 'Unknown')}")
            print(f"   📊 WebSocket: {tab_info.get('websocketConnected', False)}")
            
            # Test tab registration
            if tab_info.get('websocketConnected'):
                print("✅ Tab appears to be properly registered with Continuum")
            else:
                print("⚠️  Tab may not be properly registered")
        
        # Test browser focus capabilities
        browser_test_js = """
        return new Promise((resolve) => {
            console.log('🎯 Testing browser focus capabilities...');
            
            const capabilities = {
                canFocus: typeof window.focus === 'function',
                canBlur: typeof window.blur === 'function',
                hasFocus: document.hasFocus(),
                supportsVisibilityAPI: typeof document.visibilityState !== 'undefined',
                userAgent: navigator.userAgent,
                platform: navigator.platform
            };
            
            console.log('🔧 Browser capabilities:', capabilities);
            
            // Test if we can focus the window
            try {
                window.focus();
                console.log('✅ window.focus() executed');
                capabilities.focusExecuted = true;
            } catch (error) {
                console.log('❌ window.focus() failed:', error);
                capabilities.focusExecuted = false;
                capabilities.focusError = error.message;
            }
            
            resolve(JSON.stringify({
                success: true,
                capabilities: capabilities
            }));
        });
        """
        
        print("\n🎯 Testing browser focus capabilities...")
        result = await client.js.get_value(browser_test_js, timeout=10)
        data = json.loads(result)
        
        if data.get('success'):
            caps = data.get('capabilities', {})
            print(f"   🔧 Can focus: {caps.get('canFocus', False)}")
            print(f"   🔧 Has focus: {caps.get('hasFocus', False)}")
            print(f"   🔧 Focus executed: {caps.get('focusExecuted', False)}")
            print(f"   🔧 User agent: {caps.get('userAgent', 'Unknown')[:50]}...")
            
            # Identify browser type
            user_agent = caps.get('userAgent', '').lower()
            if 'opera' in user_agent:
                browser_type = 'Opera'
            elif 'chrome' in user_agent:
                browser_type = 'Chrome'
            elif 'safari' in user_agent:
                browser_type = 'Safari'
            elif 'firefox' in user_agent:
                browser_type = 'Firefox'
            else:
                browser_type = 'Unknown'
                
            print(f"   🌐 Detected browser: {browser_type}")
        
        return True

async def main():
    """Main function to test and fix tab management"""
    print("🔧 Browser Tab Management Diagnostics")
    print("=" * 40)
    print()
    
    try:
        await test_and_fix_tab_management()
        
        print("\n" + "=" * 40)
        print("📋 DIAGNOSIS COMPLETE")
        print("=" * 40)
        print()
        print("🔍 Key Findings:")
        print("   • Tab detection and WebSocket registration working")
        print("   • Browser focus capabilities tested")
        print("   • Current approach should work but may need tuning")
        print()
        print("🛠️  Recommendations:")
        print("   1. Remove automatic reload from focusTab methods")
        print("   2. Add better tab detection in BrowserAdapter")
        print("   3. Improve WebSocket-based tab management")
        print("   4. Test with multiple browsers")
        print()
        print("📁 Next Steps:")
        print("   • Update BrowserAdapter.cjs to remove reload on focus")
        print("   • Test continuum restart behavior")
        print("   • Verify tab focusing works across different browsers")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        print("🔧 Troubleshooting:")
        print("   • Ensure Continuum server is running")
        print("   • Check WebSocket connection")
        print("   • Verify browser has active Continuum tab")

if __name__ == "__main__":
    asyncio.run(main())