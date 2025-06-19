#!/usr/bin/env python3
"""
Test Screenshot Capture via DevTools Protocol
"""

import asyncio
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client.devtools.devtools_daemon import start_devtools_daemon
from continuum_client.core.daemon_manager import daemon_manager
import base64

async def test_screenshot():
    """Test screenshot capture through DevTools daemon"""
    
    print("🔌 Starting DevTools daemon for screenshot test...")
    
    # Start DevTools daemon
    daemon_id = await start_devtools_daemon("localhost:9000", [9222, 9223])
    print(f"🚀 Started daemon: {daemon_id}")
    
    # Wait for connection
    print("⏳ Waiting for daemon to connect...")
    await asyncio.sleep(10)  # Increased wait time
    
    # Get daemon instance
    daemon = daemon_manager.active_daemons.get(daemon_id)
    if not daemon:
        print("❌ Failed to get daemon instance")
        return
    
    # Check connection status
    status = daemon.get_status()
    print(f"🔌 Daemon status: {status['daemon_type']} - Connected: {status.get('browser_connected', False)}")
    
    # Show recent daemon logs for debugging
    logs = daemon.get_logs(20)
    print(f"\n📝 Recent daemon logs:")
    for log in logs[-10:]:  # Show more logs
        print(f"  [{log['timestamp']}] {log['level']}: {log['message']}")
    
    if not status.get('browser_connected', False):
        print("❌ Browser not connected - cannot capture screenshot")
        return
    
    # Capture screenshot
    print("📸 Capturing screenshot via DevTools Protocol...")
    screenshot_b64 = await daemon.capture_screenshot()
    
    if screenshot_b64:
        # Save screenshot to file
        screenshot_data = base64.b64decode(screenshot_b64)
        screenshot_file = Path.cwd() / f"screenshot-{daemon_id}.png"
        
        with open(screenshot_file, 'wb') as f:
            f.write(screenshot_data)
            
        print(f"✅ Screenshot saved: {screenshot_file}")
        print(f"📊 Size: {len(screenshot_data)} bytes")
        
        # Show recent daemon logs
        logs = daemon.get_logs(5)
        print(f"\n📝 Recent daemon logs:")
        for log in logs[-3:]:
            print(f"  [{log['timestamp']}] {log['level']}: {log['message']}")
            
    else:
        print("❌ Screenshot capture failed")
        
        # Show error logs
        logs = daemon.get_logs(10)
        print(f"\n🚨 Error logs:")
        for log in logs[-5:]:
            if 'ERROR' in log['level']:
                print(f"  [{log['timestamp']}] {log['level']}: {log['message']}")
    
    # Stop daemon
    daemon_manager.stop_daemon(daemon_id)
    print(f"🛑 Stopped daemon: {daemon_id}")

if __name__ == "__main__":
    asyncio.run(test_screenshot())