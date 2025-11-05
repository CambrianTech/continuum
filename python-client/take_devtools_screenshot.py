#!/usr/bin/env python3
"""Take DevTools Screenshot with Custom Name"""

import asyncio
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client.devtools.devtools_daemon import start_devtools_daemon
from continuum_client.core.daemon_manager import daemon_manager

async def capture_screenshot_named(filename):
    """Take a DevTools screenshot with custom name using existing browser"""
    print(f"📸 Taking DevTools screenshot: {filename}")
    
    # Try to use existing DevTools connection first (avoid browser multiplication)
    try:
        import aiohttp
        async with aiohttp.ClientSession() as session:
            async with session.get('http://localhost:9222/json') as response:
                if response.status == 200:
                    tabs = await response.json()
                    if tabs:
                        print("✅ Using existing DevTools connection (no new browser launch)")
                        # Use existing browser connection for screenshot
                        screenshot_path = await capture_via_existing_devtools(filename, tabs[0])
                        print(f"✅ Screenshot saved: {screenshot_path}")
                        return
    except ImportError:
        print("⚠️  aiohttp not available - using daemon fallback")
    except Exception as e:
        print(f"⚠️  Existing DevTools not available: {e}")
    
    # Fallback: Launch daemon only if no existing browser
    print("🚨 BROWSER LAUNCH: take_devtools_screenshot.py - fallback daemon launch")
    print(f"   📍 Called from: take_devtools_screenshot.py capture_screenshot_named() - FALLBACK")
    print(f"   🎯 Ports: [9222, 9223]")
    
    daemon_id = await start_devtools_daemon("localhost:9000", [9222, 9223])
    await asyncio.sleep(3)
    
    daemon = daemon_manager.active_daemons.get(daemon_id)
    if daemon and daemon.browser_connected:
        screenshot_path = await daemon.capture_screenshot(filename)
        print(f"✅ Screenshot saved: {screenshot_path}")
    else:
        print("❌ DevTools daemon not connected")
    
    daemon_manager.stop_daemon(daemon_id)

async def capture_via_existing_devtools(filename, tab_info):
    """Capture screenshot using existing DevTools connection"""
    try:
        import aiohttp
    except ImportError:
        raise Exception("aiohttp required for direct DevTools connection")
    import json
    import base64
    from pathlib import Path
    
    websocket_url = tab_info['webSocketDebuggerUrl']
    
    async with aiohttp.ClientSession() as session:
        async with session.ws_connect(websocket_url) as ws:
            # Take screenshot via DevTools protocol
            await ws.send_str(json.dumps({
                "id": 1,
                "method": "Page.captureScreenshot",
                "params": {"format": "png", "quality": 80}
            }))
            
            response = await ws.receive()
            data = json.loads(response.data)
            
            if 'result' in data and 'data' in data['result']:
                # Decode and save screenshot
                screenshot_data = base64.b64decode(data['result']['data'])
                screenshot_path = Path('.continuum/screenshots') / f'{filename}.png'
                screenshot_path.parent.mkdir(parents=True, exist_ok=True)
                screenshot_path.write_bytes(screenshot_data)
                return str(screenshot_path)
            else:
                raise Exception(f"Screenshot failed: {data}")

if __name__ == "__main__":
    filename = sys.argv[1] if len(sys.argv) > 1 else "continuum_interface_capture"
    asyncio.run(capture_screenshot_named(filename))