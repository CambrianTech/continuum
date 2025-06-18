#!/usr/bin/env python3
"""
Full Page Debug Screenshot
Capture entire page to see what user sees and debug issues
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def full_page_debug():
    print("📸 FULL PAGE DEBUG - Capturing entire page to see what you see")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'full-page-debug',
            'agentName': 'Full Page Debug',
            'agentType': 'ai'
        })
        
        # Capture ENTIRE page using body element
        result = await client.js.execute("""
            console.log('📸 FULL PAGE DEBUG: Capturing entire document.body');
            console.log('📸 Page dimensions:', window.innerWidth + 'x' + window.innerHeight);
            console.log('📸 Document dimensions:', document.body.scrollWidth + 'x' + document.body.scrollHeight);
            
            // Debug: Show what elements exist
            console.log('📋 Available elements:');
            const userElements = document.querySelectorAll('[class*="user"], [id*="user"]');
            console.log('  - User elements:', userElements.length);
            userElements.forEach((el, i) => {
                console.log('    ' + i + ':', el.className, el.offsetWidth + 'x' + el.offsetHeight);
            });
            
            const agentElements = document.querySelectorAll('[class*="agent"], [id*="agent"]');
            console.log('  - Agent elements:', agentElements.length);
            agentElements.forEach((el, i) => {
                console.log('    ' + i + ':', el.className, el.offsetWidth + 'x' + el.offsetHeight);
            });
            
            const projectElements = document.querySelectorAll('[class*="project"], [id*="project"]');
            console.log('  - Project elements:', projectElements.length);
            
            if (typeof html2canvas !== 'undefined' && window.ws) {
                html2canvas(document.body, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 0.5,  // Reduce scale for full page
                    backgroundColor: '#1a1a1a',
                    height: window.innerHeight,
                    width: window.innerWidth
                }).then(function(canvas) {
                    console.log('📸 Full page canvas created:', canvas.width + 'x' + canvas.height);
                    var dataURL = canvas.toDataURL('image/png');
                    var timestamp = Date.now();
                    var filename = 'full_page_debug_' + timestamp + '.png';
                    
                    if (window.ws.readyState === 1) {
                        window.ws.send(JSON.stringify({
                            type: 'screenshot_data',
                            dataURL: dataURL,
                            filename: filename,
                            timestamp: timestamp,
                            debug: 'full_page_capture'
                        }));
                        console.log('✅ Full page screenshot sent via WebSocket');
                    }
                });
                return 'FULL_PAGE_SENT';
            } else {
                return 'MISSING_DEPENDENCIES';
            }
        """)
        
        print(f'Full page result: {result}')
        
        import time
        time.sleep(3)  # More time for full page
        
        # Check files
        screenshot_dir = Path('.continuum/screenshots')
        if screenshot_dir.exists():
            files = list(screenshot_dir.glob('full_page_debug_*.png'))
            print(f'Full page files: {len(files)}')
            for f in files:
                print(f'  - {f.name} ({f.stat().st_size} bytes)')
                # Auto-open the full page screenshot
                import subprocess
                subprocess.run(['open', str(f)], check=False)
                print(f'🖼️ Opened: {f.name}')
        else:
            print('❌ No screenshot directory found')

asyncio.run(full_page_debug())