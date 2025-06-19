#!/usr/bin/env python3
"""Take DevTools Screenshot with Custom Name"""

import asyncio
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client.devtools.devtools_daemon import start_devtools_daemon
from continuum_client.core.daemon_manager import daemon_manager

async def capture_screenshot_named(filename):
    """Take a DevTools screenshot with custom name"""
    print(f"📸 Taking DevTools screenshot: {filename}")
    
    daemon_id = await start_devtools_daemon("localhost:9000", [9222, 9223])
    await asyncio.sleep(3)
    
    daemon = daemon_manager.active_daemons.get(daemon_id)
    if daemon and daemon.browser_connected:
        screenshot_path = await daemon.capture_screenshot(filename)
        print(f"✅ Screenshot saved: {screenshot_path}")
    else:
        print("❌ DevTools daemon not connected")
    
    daemon_manager.stop_daemon(daemon_id)

if __name__ == "__main__":
    filename = sys.argv[1] if len(sys.argv) > 1 else "continuum_interface_capture"
    asyncio.run(capture_screenshot_named(filename))