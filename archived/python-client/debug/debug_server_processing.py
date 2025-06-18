#!/usr/bin/env python3
"""
Debug Server Processing
Test screenshot while monitoring server logs to identify where file saving fails
"""

import asyncio
import sys
import time
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def debug_server_processing():
    print("🔍 DEBUGGING SERVER-SIDE SCREENSHOT PROCESSING")
    print("Monitor server logs for screenshot_data message handling")
    print("=" * 60)
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'debug-server-processing',
            'agentName': 'Debug Server Processing',
            'agentType': 'ai'
        })
        
        print("🧪 Triggering screenshot with detailed server monitoring...")
        print("📡 Watch server logs for:")
        print("  - 📨 RAW MESSAGE RECEIVED: screenshot_data")
        print("  - 📋 PARSED MESSAGE TYPE: screenshot_data") 
        print("  - 📸 PROCESSING SCREENSHOT_DATA MESSAGE")
        print("  - ✅ Screenshot saved directly")
        print("")
        
        # Test using the universal command API which should trigger the consolidated utils
        print("🚀 Sending SCREENSHOT command...")
        result = await client.command.screenshot(
            selector='.version-badge',
            name_prefix='server_debug_test',
            scale=1.0,
            manual=False
        )
        
        print(f"📊 Command result: {result}")
        print("")
        print("⏳ Waiting 5 seconds for server processing...")
        
        # Wait and monitor for file creation
        for i in range(5):
            time.sleep(1)
            print(f"  ⏰ {i+1}/5 seconds...")
            
            # Check if file was created
            screenshot_dir = Path('.continuum/screenshots')
            if screenshot_dir.exists():
                files = list(screenshot_dir.glob('server_debug_test_*.png'))
                if files:
                    for f in files:
                        size = f.stat().st_size
                        print(f"  ✅ FILE CREATED: {f.name} ({size} bytes)")
                        if size > 1000:
                            print("  🎉 SUCCESS: Server processing working!")
                            return True
        
        print("")
        print("💥 ANALYSIS:")
        print("  If you see in server logs:")
        print("  - '📨 RAW MESSAGE RECEIVED: screenshot_data' = Message arrived ✅")
        print("  - '📋 PARSED MESSAGE TYPE: screenshot_data' = Message parsed ✅") 
        print("  - '📸 PROCESSING SCREENSHOT_DATA MESSAGE' = Handler called ✅")
        print("  - '✅ Screenshot saved directly' = File should be saved ✅")
        print("")
        print("  If missing any of above = Server processing broken ❌")
        print("  If all present but no file = File path/permissions issue ❌")
        
        return False

if __name__ == "__main__":
    success = asyncio.run(debug_server_processing())
    if success:
        print("\n🎉 Server processing WORKING!")
    else:
        print("\n💥 Server processing FAILED - check server logs for missing steps")