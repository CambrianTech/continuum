#!/usr/bin/env python3
"""
Test Permanent ignoreElements Fix
Verify the permanent fix in continuum-api.js is working
"""

import asyncio
import sys
import time
from pathlib import Path

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def test_permanent_fix():
    print("🧪 Testing permanent ignoreElements fix")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'test-permanent-fix',
            'agentName': 'Test Permanent Fix',
            'agentType': 'ai'
        })
        
        print("🧪 Testing screenshot with permanent fix...")
        
        # Test using the universal command API
        result = await client.command.screenshot(
            selector='.version-badge',
            name_prefix='permanent_fix_test',
            scale=1.0,
            manual=False
        )
        
        print(f"📊 Screenshot result: {result}")
        
        # Wait for file creation
        print("⏳ Waiting 3 seconds for file creation...")
        time.sleep(3)
        
        # Check if file was created
        screenshot_dir = Path('.continuum/screenshots')
        if screenshot_dir.exists():
            files = list(screenshot_dir.glob('permanent_fix_test_*.png'))
            if files:
                for f in files:
                    size = f.stat().st_size
                    print(f"✅ SUCCESS: {f.name} ({size} bytes)")
                    if size > 1000:  # More than 1KB indicates real success
                        print("🎉 Permanent fix is working!")
                        
                        # Open the screenshot
                        import subprocess
                        subprocess.run(['open', str(f)], check=False)
                        
                        return True
            else:
                print("❌ No permanent_fix_test files created")
        else:
            print("❌ Screenshot directory not found")
            
        return False

if __name__ == "__main__":
    success = asyncio.run(test_permanent_fix())
    if success:
        print("\n🎉 PERMANENT FIX VERIFIED - createPattern error resolved!")
    else:
        print("\n💥 Permanent fix failed - needs investigation")