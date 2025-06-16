#!/usr/bin/env python3
"""
Test Browser API Direct
Test the browser-side continuum.command.screenshot() API directly
"""

import asyncio
import sys
import time
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def test_browser_api_direct():
    print("🧪 Testing browser-side continuum API directly")
    print("This should use the consolidated ScreenshotUtils")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'test-browser-api-direct',
            'agentName': 'Test Browser API Direct',
            'agentType': 'ai'
        })
        
        # Test the browser-side continuum.command.screenshot() directly
        result = await client.js.execute("""
            console.log('🧪 Testing continuum.command.screenshot() directly...');
            
            if (typeof window.continuum === 'undefined') {
                return 'NO_CONTINUUM_API';
            }
            
            if (typeof window.continuum.command === 'undefined') {
                return 'NO_COMMAND_API';
            }
            
            if (typeof window.continuum.command.screenshot !== 'function') {
                return 'NO_SCREENSHOT_FUNCTION';
            }
            
            console.log('✅ continuum.command.screenshot exists, calling it...');
            
            try {
                var promise = window.continuum.command.screenshot({
                    selector: '.version-badge',
                    name_prefix: 'browser_api_direct_test',
                    scale: 1.0,
                    manual: false
                });
                
                console.log('📸 Screenshot promise created:', typeof promise);
                
                if (promise && typeof promise.then === 'function') {
                    promise.then(function(result) {
                        console.log('✅ Screenshot promise resolved:', result);
                        return 'PROMISE_RESOLVED';
                    }).catch(function(error) {
                        console.error('❌ Screenshot promise rejected:', error);
                        return 'PROMISE_REJECTED: ' + error.message;
                    });
                    
                    return 'PROMISE_CREATED';
                } else {
                    return 'NO_PROMISE_RETURNED';
                }
                
            } catch (error) {
                console.error('❌ Exception calling screenshot:', error.message);
                return 'EXCEPTION: ' + error.message;
            }
        """)
        
        print(f"Browser API test result: {result}")
        
        # Wait to see if file gets created
        print("⏳ Waiting 5 seconds for file creation...")
        time.sleep(5)
        
        # Check for file
        screenshot_dir = Path('.continuum/screenshots')
        if screenshot_dir.exists():
            files = list(screenshot_dir.glob('browser_api_direct_test_*.png'))
            if files:
                for f in files:
                    size = f.stat().st_size
                    print(f"✅ FILE CREATED: {f.name} ({size} bytes)")
                    return True
            else:
                print("❌ No files created")
        else:
            print("❌ Screenshot directory not found")
            
        return False

if __name__ == "__main__":
    success = asyncio.run(test_browser_api_direct())
    if success:
        print("\n🎉 Browser API working!")
    else:
        print("\n💥 Browser API failed!")