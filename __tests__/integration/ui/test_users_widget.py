#!/usr/bin/env python3
"""
Test Users Widget Screenshot
Use exact same approach as working version badge but target users widget
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def test_users_widget():
    print("🔍 Testing Users Widget Screenshot")
    print("Using EXACT same approach as working version badge")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'users-widget-test',
            'agentName': 'Users Widget Test',
            'agentType': 'ai'
        })
        
        # Use proper screenshot command instead of direct html2canvas
        print("📸 Using proper screenshot command for users widget")
        
        # Try multiple selectors for users widget
        selectors = [
            '[class*="user"]',
            '[class*="agent"]', 
            '[id*="user"]',
            '[id*="agent"]',
            '#sidebar',
            '.sidebar',
            'body'  # fallback
        ]
        
        result = None
        for selector in selectors:
            try:
                result = await client.js.execute(f"""
                    if (window.continuum && window.continuum.command && window.continuum.command.screenshot) {{
                        console.log('📸 Using continuum.command.screenshot for: {selector}');
                        return window.continuum.command.screenshot({{
                            selector: '{selector}',
                            name_prefix: 'users_widget_test',
                            scale: 2.0,
                            manual: false
                        }});
                    }} else {{
                        console.log('❌ Screenshot command not available');
                        return 'NO_SCREENSHOT_COMMAND';
                    }}
                """)
                
                if result and result.get('success'):
                    print(f"✅ Screenshot successful with selector: {selector}")
                    break
                else:
                    print(f"❌ Screenshot failed with selector: {selector}")
                    
            except Exception as e:
                print(f"❌ Error with selector {selector}: {e}")
                continue
        
        print(f'Result: {result}')
        
        import time
        time.sleep(2)
        
        # Check files
        screenshot_dir = Path('.continuum/screenshots')
        if screenshot_dir.exists():
            files = list(screenshot_dir.glob('users_widget_*.png'))
            print(f'Users widget files: {len(files)}')
            for f in files:
                print(f'  - {f.name} ({f.stat().st_size} bytes)')
                # Open the file to verify it worked
                import subprocess
                subprocess.run(['open', str(f)], check=False)
        else:
            print('No screenshot directory found')

asyncio.run(test_users_widget())