#!/usr/bin/env python3
"""
Test Screenshot with Simple JavaScript
Use simple JS calls to avoid syntax errors
"""

import asyncio
import sys
import time
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def test_screenshot_simple():
    print("🧪 Testing screenshot with simple JavaScript")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'test-screenshot-simple',
            'agentName': 'Test Screenshot Simple',
            'agentType': 'ai'
        })
        
        # Test 1: Check version element exists
        print("\n--- TEST 1: Check version element ---")
        result1 = await client.js.execute("var el = document.querySelector('.version-badge'); return el ? el.offsetWidth + 'x' + el.offsetHeight : 'not found';")
        print(f"Version element: {result1}")
        
        # Test 2: Use the working continuum API directly
        print("\n--- TEST 2: Use continuum.command.screenshot() ---")
        result2 = await client.js.execute("return window.continuum && window.continuum.command && window.continuum.command.screenshot ? 'available' : 'not available';")
        print(f"Continuum API: {result2}")
        
        if result2.get('result') == 'available':
            # Test 3: Try calling the API (this might timeout but should give us more info)
            print("\n--- TEST 3: Call continuum.command.screenshot() ---")
            try:
                # Call the browser-side API directly
                api_result = await client.js.execute("""
                    window.continuum.command.screenshot({
                        selector: '.version-badge',
                        name_prefix: 'simple_test',
                        scale: 1.0,
                        manual: false
                    });
                    return 'screenshot_called';
                """)
                print(f"API call result: {api_result}")
                
                # Wait for file creation
                print("⏳ Waiting 3 seconds for file creation...")
                time.sleep(3)
                
                # Check if file was created
                screenshot_dir = Path('.continuum/screenshots')
                if screenshot_dir.exists():
                    files = list(screenshot_dir.glob('simple_test_*.png'))
                    if files:
                        for f in files:
                            size = f.stat().st_size
                            print(f"✅ Created: {f.name} ({size} bytes)")
                            if size > 1000:  # More than 1KB indicates success
                                print("🎉 Screenshot appears successful!")
                                return True
                    else:
                        print("❌ No simple_test files created")
                else:
                    print("❌ Screenshot directory not found")
                    
            except Exception as e:
                print(f"API call failed: {e}")
        
        return False

if __name__ == "__main__":
    success = asyncio.run(test_screenshot_simple())
    if success:
        print("\n🎉 Simple screenshot test PASSED!")
    else:
        print("\n💥 Simple screenshot test FAILED!")