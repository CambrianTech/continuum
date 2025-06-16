#!/usr/bin/env python3
"""
Test Users Widget Screenshot
Use exact same approach as working version badge but target users widget
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

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
        
        # Use EXACT same JavaScript as working version badge but target users widget
        result = await client.js.execute("""
            console.log('📸 WORKING PIPELINE: Capturing users widget');
            
            // Find users widget using multiple selectors
            const selectors = [
                '[class*="user"]',
                '[class*="agent"]', 
                '[id*="user"]',
                '[id*="agent"]',
                '#sidebar',
                '.sidebar',
                'body'  // fallback
            ];
            
            let targetElement = null;
            let targetSelector = null;
            
            for (const selector of selectors) {
                const element = document.querySelector(selector);
                if (element && element.offsetWidth > 0 && element.offsetHeight > 0) {
                    targetElement = element;
                    targetSelector = selector;
                    console.log('📸 Found target:', selector, element.offsetWidth + 'x' + element.offsetHeight);
                    break;
                }
            }
            
            if (targetElement) {
                html2canvas(targetElement, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 2,
                    backgroundColor: '#1a1a1a'
                }).then(function(canvas) {
                    var dataURL = canvas.toDataURL('image/png');
                    var timestamp = Date.now();
                    var filename = 'users_widget_' + timestamp + '.png';
                    
                    if (window.ws && window.ws.readyState === 1) {
                        window.ws.send(JSON.stringify({
                            type: 'screenshot_data',
                            dataURL: dataURL,
                            filename: filename,
                            timestamp: timestamp
                        }));
                        console.log('✅ Users widget sent via WebSocket');
                    }
                });
                return 'SUCCESS';
            } else {
                return 'ELEMENT_NOT_FOUND';
            }
        """)
        
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