#!/usr/bin/env python3
"""
Debug with Scale - Low Resolution Full Page
Use scale parameter to reduce resolution while debugging WebSocket processing
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def debug_with_scale():
    print("📸 DEBUG WITH SCALE - Low resolution full page capture")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'debug-scale',
            'agentName': 'Debug Scale',
            'agentType': 'ai'
        })
        
        # Test WebSocket message processing with lower resolution
        result = await client.js.execute("""
            console.log('📸 DEBUG SCALE: Testing WebSocket message processing');
            
            if (typeof html2canvas !== 'undefined' && window.ws) {
                console.log('📸 WebSocket state:', window.ws.readyState);
                console.log('📸 Using scale 0.2 (20% resolution)');
                
                html2canvas(document.body, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 0.2,  // 20% resolution
                    backgroundColor: '#1a1a1a'
                }).then(function(canvas) {
                    console.log('📸 Low-res canvas:', canvas.width + 'x' + canvas.height);
                    var dataURL = canvas.toDataURL('image/png');
                    var timestamp = Date.now();
                    var filename = 'debug_scale_' + timestamp + '.png';
                    
                    // Debug the WebSocket message
                    const message = {
                        type: 'screenshot_data',
                        dataURL: dataURL,
                        filename: filename,
                        timestamp: timestamp,
                        debug: 'scale_test'
                    };
                    
                    console.log('📤 Sending WebSocket message:', {
                        type: message.type,
                        filename: message.filename,
                        dataSize: message.dataURL.length,
                        timestamp: message.timestamp
                    });
                    
                    window.ws.send(JSON.stringify(message));
                    console.log('✅ WebSocket send completed');
                    
                    // Also try to trigger immediate feedback
                    setTimeout(() => {
                        console.log('📋 WebSocket send aftermath check');
                    }, 100);
                });
                return 'SCALE_TEST_SENT';
            } else {
                return 'DEPENDENCIES_MISSING';
            }
        """)
        
        print(f'Scale test result: {result}')
        
        import time
        time.sleep(2)
        
        # Check files
        screenshot_dir = Path('.continuum/screenshots')
        if screenshot_dir.exists():
            files = list(screenshot_dir.glob('debug_scale_*.png'))
            print(f'Scale test files: {len(files)}')
            for f in files:
                print(f'  - {f.name} ({f.stat().st_size} bytes)')
                import subprocess
                subprocess.run(['open', str(f)], check=False)
        else:
            print('❌ No screenshot directory found')

asyncio.run(debug_with_scale())