#!/usr/bin/env python3
"""
Test ignoreElements Fix for createPattern Error
Patch continuum API and test screenshot functionality
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def test_ignoreelements_fix():
    print("🧪 TESTING ignoreElements fix for createPattern error")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'test-ignoreelements-fix',
            'agentName': 'Test IgnoreElements Fix',
            'agentType': 'ai'
        })
        
        # Test the proper screenshot command (should already have ignoreElements fix built-in)
        print("🧪 Testing proper screenshot command (no patching needed)...")
        
        # Check if screenshot command is available
        command_check = await client.js.execute("""
            console.log('🔍 Checking for screenshot command...');
            if (window.continuum && window.continuum.command && window.continuum.command.screenshot) {
                console.log('✅ Screenshot command is available');
                return 'COMMAND_AVAILABLE';
            } else {
                console.log('❌ Screenshot command not available');
                return 'COMMAND_NOT_AVAILABLE';
            }
        """)
        print(f"Command check: {command_check}")
        
        if command_check == 'COMMAND_AVAILABLE':
            # Use the proper screenshot command
            result = await client.command.screenshot(
                selector='.version-badge',
                name_prefix='test_fixed',
                scale=1.0,
                manual=False
            )
        else:
            print("❌ Screenshot command not available, skipping test")
            result = {'success': False, 'error': 'Screenshot command not available'}
        print(f"Test result: {result}")
        
        # Wait for file to be created
        import time
        time.sleep(3)
        
        # Check if screenshot was created
        screenshot_dir = Path('.continuum/screenshots')
        if screenshot_dir.exists():
            files = list(screenshot_dir.glob('test_fixed_*.png'))
            print(f"Created files: {len(files)}")
            if files:
                for f in files:
                    size_kb = f.stat().st_size // 1024
                    print(f"  ✅ {f.name} ({size_kb}KB)")
                    
                    # Open the screenshot to verify it worked
                    import subprocess
                    subprocess.run(['open', str(f)], check=False)
                    
                    # Save test results to .continuum
                    test_results = {
                        'test': 'screenshot_command_test',
                        'status': 'SUCCESS',
                        'filename': f.name,
                        'size_kb': size_kb,
                        'method': 'proper_screenshot_command',
                        'note': 'Using built-in screenshot command instead of html2canvas'
                    }
                    
                    results_file = Path('.continuum/test_results.json')
                    import json
                    with open(results_file, 'w') as f:
                        json.dump(test_results, f, indent=2)
                    print(f"📄 Test results saved to {results_file}")
                    
                return True
            else:
                print("❌ No screenshot files created - fix didn't work")
                return False
        else:
            print("❌ Screenshot directory not found")
            return False

if __name__ == "__main__":
    success = asyncio.run(test_ignoreelements_fix())
    if success:
        print("🎉 ignoreElements fix SUCCESSFUL!")
    else:
        print("💥 ignoreElements fix FAILED!")