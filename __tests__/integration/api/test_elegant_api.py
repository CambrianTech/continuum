#!/usr/bin/env python3
"""
Test Elegant Universal API
continuum.command.screenshot() pattern
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def test_elegant_api():
    print("🎯 Testing Universal Elegant API")
    print("=" * 50)
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'elegant-api-test',
            'agentName': 'Elegant API Test',
            'agentType': 'ai'
        })
        
        print("\n📸 Test 1: Basic screenshot")
        result1 = await client.command.screenshot()
        print(f"Result: {result1}")
        
        print("\n📸 Test 2: Version badge screenshot")
        result2 = await client.command.screenshot(
            selector='.version-badge',
            name_prefix='version_test',
            scale=2.0
        )
        print(f"Result: {result2}")
        
        print("\n📸 Test 3: Manual mode")
        result3 = await client.command.screenshot(
            selector='body',
            name_prefix='manual_test',
            manual=True
        )
        print(f"Result: {result3}")
        
        print("\n✅ All elegant API tests completed")

if __name__ == "__main__":
    asyncio.run(test_elegant_api())