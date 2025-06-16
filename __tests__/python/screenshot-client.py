#!/usr/bin/env python3
"""
Simple screenshot test
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config
from continuum_client.utils.screenshot import capture_version_badge

async def main():
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'simple-test',
            'agentName': 'Simple Screenshot Test',
            'agentType': 'ai'
        })
        
        print("Testing version screenshot...")
        result = await capture_version_badge(client)
        print(f'Version result: {result}')

if __name__ == "__main__":
    asyncio.run(main())