#!/usr/bin/env python3
"""
Check Browser Cache - Verify browser is using updated continuum-api.js
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def check_browser_cache():
    print("🔍 Checking if browser is using updated continuum-api.js")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'check-browser-cache',
            'agentName': 'Check Browser Cache',
            'agentType': 'ai'
        })
        
        # Check if the ignoreElements fix is in the browser's continuum API
        result = await client.js.execute("""
            var apiCode = window.continuum.command.screenshot.toString();
            var hasIgnoreElements = apiCode.includes('ignoreElements');
            var hasZeroDimension = apiCode.includes('offsetWidth === 0');
            
            console.log('🔍 Checking continuum API for fix...');
            console.log('Has ignoreElements:', hasIgnoreElements);
            console.log('Has zero dimension check:', hasZeroDimension);
            
            if (hasIgnoreElements && hasZeroDimension) {
                return 'FIX_PRESENT';
            } else {
                return 'FIX_MISSING';
            }
        """)
        
        print(f"Browser cache check: {result}")
        
        if result.get('result') == 'FIX_MISSING':
            print("❌ Browser is using OLD version of continuum-api.js")
            print("💡 Need to force browser cache refresh")
        else:
            print("✅ Browser has the ignoreElements fix")

if __name__ == "__main__":
    asyncio.run(check_browser_cache())