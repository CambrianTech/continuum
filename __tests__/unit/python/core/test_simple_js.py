#!/usr/bin/env python3
"""
Test Simple JavaScript Execution
Test basic JavaScript execution without complex syntax
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def test_simple_js():
    print("🧪 Testing simple JavaScript execution")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'test-simple-js',
            'agentName': 'Test Simple JS',
            'agentType': 'ai'
        })
        
        # Test 1: Very simple JavaScript
        print("\n--- TEST 1: Simple return ---")
        result1 = await client.js.execute("return 'hello world';")
        print(f"Result 1: {result1}")
        
        # Test 2: Console log
        print("\n--- TEST 2: Console log ---")
        result2 = await client.js.execute("console.log('test message'); return 'logged';")
        print(f"Result 2: {result2}")
        
        # Test 3: Simple variable
        print("\n--- TEST 3: Simple variable ---")
        result3 = await client.js.execute("var x = 42; return x;")
        print(f"Result 3: {result3}")
        
        # Test 4: Check html2canvas availability (simple)
        print("\n--- TEST 4: Check html2canvas ---")
        result4 = await client.js.execute("return typeof html2canvas;")
        print(f"Result 4: {result4}")
        
        # Test 5: Simple async function (might fail)
        print("\n--- TEST 5: Simple async (might fail) ---")
        try:
            result5 = await client.js.execute("return (async () => { return 'async test'; })();")
            print(f"Result 5: {result5}")
        except Exception as e:
            print(f"Result 5 FAILED: {e}")
            
        return True

if __name__ == "__main__":
    asyncio.run(test_simple_js())