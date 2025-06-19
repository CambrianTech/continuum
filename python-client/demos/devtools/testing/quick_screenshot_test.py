#!/usr/bin/env python3
"""Quick DevTools Screenshot Test"""

import asyncio
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client.devtools.devtools_daemon import start_devtools_daemon
from continuum_client.core.daemon_manager import daemon_manager

async def quick_screenshot():
    """Take a quick screenshot via DevTools"""
    print("📸 Taking DevTools screenshot...")
    
    # Start DevTools daemon 
    daemon_id = await start_devtools_daemon("localhost:9000", [9222, 9223])
    
    await asyncio.sleep(3)  # Wait for connection
    
    # Get daemon and capture screenshot
    daemon = daemon_manager.active_daemons.get(daemon_id)
    if daemon and daemon.browser_connected:
        screenshot_path = await daemon.capture_screenshot("portal_integration_working")
        print(f"✅ Screenshot captured: {screenshot_path}")
        
        # Show recent logs
        print("\n📋 Recent browser activity:")
        if hasattr(daemon, 'log_count'):
            print(f"   📊 Total logs captured: {daemon.log_count}")
    else:
        print("❌ DevTools daemon not connected")
    
    # Cleanup
    daemon_manager.stop_daemon(daemon_id)
    print("🎉 Screenshot test completed!")

if __name__ == "__main__":
    asyncio.run(quick_screenshot())