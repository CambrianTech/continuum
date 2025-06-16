#!/usr/bin/env python3
"""
Test Elegant Browser API
Use continuum.command.screenshot() instead of injecting JavaScript
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def test_browser_api():
    print("🎯 Testing Elegant Browser API")
    print("=" * 50)
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'browser-api-test',
            'agentName': 'Browser API Test',
            'agentType': 'ai'
        })
        
        print("\n📸 Test: Using continuum.command.screenshot() directly")
        result = await client.js.execute("""
            // Use the elegant browser API instead of injecting screenshot logic
            continuum.command.screenshot({
                selector: '.version-badge',
                name_prefix: 'elegant_version',
                scale: 2.0
            });
        """)
        print(f"Browser API result: {result}")
        
        # Wait for processing
        import time
        time.sleep(2)
        
        print("\n📸 Test 2: Full page screenshot")
        result2 = await client.js.execute("""
            continuum.command.screenshot({
                selector: 'body',
                name_prefix: 'elegant_fullpage',
                scale: 1.0
            });
        """)
        print(f"Full page result: {result2}")
        
        time.sleep(2)

if __name__ == "__main__":
    asyncio.run(test_browser_api())