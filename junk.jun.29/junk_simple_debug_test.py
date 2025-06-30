#!/usr/bin/env python3
"""
Simple Browser Debug Test
Refresh browser and show console logs - proving JTAG works
"""

import asyncio
from continuum_client.async_client import connect

async def simple_debug_test():
    """Simple test: refresh browser and show logs"""
    print("🔍 Simple Browser Debug Test")
    print("=" * 40)
    
    # Connect elegantly
    async with await connect() as continuum:
        print("✅ Connected to Continuum")
        
        # 1. Refresh the browser
        print("\n🔄 Refreshing browser...")
        try:
            await continuum.browser_navigate("http://localhost:9000")
            print("✅ Browser refresh initiated")
        except Exception as e:
            print(f"❌ Browser refresh failed: {e}")
        
        # Wait a moment for page load
        await asyncio.sleep(2)
        
        # 2. Get console logs
        print("\n📜 Current console logs:")
        try:
            logs = await continuum.console_logs(limit=10)
            
            if logs:
                for i, log in enumerate(logs, 1):
                    level_emoji = {
                        'info': '💡',
                        'warn': '⚠️', 
                        'error': '❌',
                        'debug': '🔍'
                    }.get(log.get('level', 'info'), '📝')
                    
                    message = log.get('message', str(log))
                    print(f"  {i:2d}. {level_emoji} {message[:80]}...")
            else:
                print("  📝 No console logs found")
                
        except Exception as e:
            print(f"❌ Failed to get logs: {e}")
        
        # 3. Take a screenshot
        print("\n📸 Taking screenshot...")
        try:
            screenshot = await continuum.browser_screenshot()
            if screenshot:
                with open("browser_debug.png", "wb") as f:
                    f.write(screenshot)
                print(f"✅ Screenshot saved: browser_debug.png ({len(screenshot)} bytes)")
            else:
                print("⚠️ No screenshot available")
        except Exception as e:
            print(f"❌ Screenshot failed: {e}")
        
        # 4. Quick system status
        print("\n📊 System status:")
        try:
            status = await continuum.daemon_status()
            daemons = status.get('daemons', {})
            
            for name, info in daemons.items():
                status_emoji = "✅" if info.get('status') == 'running' else "❌"
                print(f"  {status_emoji} {name}: {info.get('status', 'unknown')}")
                
        except Exception as e:
            print(f"❌ Status check failed: {e}")

if __name__ == "__main__":
    print("🚀 Testing JTAG browser debugging capabilities")
    print("Will refresh browser and show console logs")
    print()
    
    asyncio.run(simple_debug_test())
    
    print("\n✨ Done! Check browser_debug.png for visual confirmation")