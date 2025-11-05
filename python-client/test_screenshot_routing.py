#!/usr/bin/env python3
"""
Test Screenshot Path Routing
Demonstrates how daemon respects configured screenshot directories
"""

import asyncio
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client.devtools.devtools_daemon import start_devtools_daemon
from continuum_client.core.daemon_manager import daemon_manager

async def test_screenshot_routing():
    """Test screenshot capture with different directory configurations"""
    
    print("📸 Testing DevTools daemon screenshot path routing...")
    
    # Test 1: Default routing (should use Continuum's .continuum/screenshots)
    print("\n🎯 Test 1: Default routing (Continuum screenshot directory)")
    daemon_id_1 = await start_devtools_daemon("localhost:9000", [9222, 9223])
    
    await asyncio.sleep(3)  # Wait for connection
    
    daemon_1 = daemon_manager.active_daemons.get(daemon_id_1)
    if daemon_1 and daemon_1.browser_connected:
        screenshot_path_1 = await daemon_1.capture_screenshot("test_default_routing")
        print(f"✅ Default routing screenshot: {screenshot_path_1}")
    else:
        print("❌ Daemon 1 not connected")
    
    # Test 2: Custom directory routing
    print("\n🎯 Test 2: Custom directory routing")
    custom_dir = "/Users/joel/Development/cambrian/continuum/python-client/custom_screenshots"
    daemon_id_2 = await start_devtools_daemon("localhost:9000", [9222, 9223], 
                                            screenshot_dir=custom_dir)
    
    await asyncio.sleep(3)  # Wait for connection
    
    daemon_2 = daemon_manager.active_daemons.get(daemon_id_2)
    if daemon_2 and daemon_2.browser_connected:
        screenshot_path_2 = await daemon_2.capture_screenshot("test_custom_routing")
        print(f"✅ Custom routing screenshot: {screenshot_path_2}")
    else:
        print("❌ Daemon 2 not connected")
    
    # Test 3: Different formats and quality
    print("\n🎯 Test 3: Format and quality options")
    if daemon_1 and daemon_1.browser_connected:
        # PNG (default)
        png_path = await daemon_1.capture_screenshot("test_png_format", format="png")
        print(f"✅ PNG screenshot: {png_path}")
        
        # JPEG with quality
        jpeg_path = await daemon_1.capture_screenshot("test_jpeg_format", format="jpeg", quality=75)
        print(f"✅ JPEG screenshot: {jpeg_path}")
    
    # Show directory structures
    print("\n📁 Directory structures created:")
    
    # Check default directory
    continuum_screenshots = Path("/Users/joel/Development/cambrian/continuum/.continuum/screenshots")
    if continuum_screenshots.exists():
        print(f"📂 Continuum screenshots: {continuum_screenshots}")
        screenshots = list(continuum_screenshots.glob("*.png")) + list(continuum_screenshots.glob("*.jpeg"))
        for screenshot in screenshots[-3:]:  # Show last 3
            print(f"   📸 {screenshot.name}")
    
    # Check custom directory
    custom_screenshots = Path(custom_dir)
    if custom_screenshots.exists():
        print(f"📂 Custom screenshots: {custom_screenshots}")
        screenshots = list(custom_screenshots.glob("*.png")) + list(custom_screenshots.glob("*.jpeg"))
        for screenshot in screenshots:
            print(f"   📸 {screenshot.name}")
    
    # Show daemon status and configuration
    print("\n🔧 Daemon configurations:")
    for daemon_id, daemon in daemon_manager.active_daemons.items():
        if hasattr(daemon, 'screenshot_dir'):
            print(f"🤖 {daemon_id}: {daemon.screenshot_dir}")
    
    # Cleanup
    daemon_manager.stop_daemon(daemon_id_1)
    if daemon_id_2 != daemon_id_1:  # In case they reused the same daemon
        daemon_manager.stop_daemon(daemon_id_2)
    
    print("\n🎉 Screenshot routing test completed!")

if __name__ == "__main__":
    asyncio.run(test_screenshot_routing())