#!/usr/bin/env python3
"""
Run Console Check
Execute the console warning check script
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def run_console_check():
    print("🔍 RUNNING CONSOLE CHECK")
    
    # Read the JS file
    js_file = Path(__file__).parent / 'browser_scripts' / 'check_console_warnings.js'
    js_content = js_file.read_text()
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'run-console-check',
            'agentName': 'Run Console Check',
            'agentType': 'ai'
        })
        
        # Execute the script
        result = await client.js.execute(js_content)
        print(f"Script execution result: {result}")
        
        # Wait for the script to complete
        await asyncio.sleep(3)
        
        # Check final state
        final_check = await client.js.execute("""
            return {
                continuum: typeof window.continuum !== 'undefined',
                initFunc: typeof initializeContinuum === 'function',
                screenshots: typeof window.ScreenshotUtils !== 'undefined'
            };
        """)
        
        print(f"Final check: {final_check}")

if __name__ == "__main__":
    asyncio.run(run_console_check())