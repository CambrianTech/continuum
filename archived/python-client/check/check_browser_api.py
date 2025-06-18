#!/usr/bin/env python3
"""
Check Browser API Status
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def check_api():
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({'agentId': 'api-check', 'agentName': 'API Check', 'agentType': 'ai'})
        
        print("Checking browser API availability...")
        
        result = await client.js.execute('typeof window.continuum')
        print(f'continuum type: {result}')
        
        result2 = await client.js.execute('Object.keys(window.continuum || {})')
        print(f'continuum keys: {result2}')
        
        result3 = await client.js.execute('window.continuum?.command ? "command available" : "command missing"')
        print(f'command status: {result3}')

asyncio.run(check_api())