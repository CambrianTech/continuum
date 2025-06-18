#!/usr/bin/env python3
"""
Debug createPattern Error
Add extensive logging to track down the html2canvas failure
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def debug_createpattern():
    print("🔍 DEBUGGING createPattern ERROR")
    print("Adding extensive logging to track html2canvas failure")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'debug-createpattern',
            'agentName': 'Debug CreatePattern',
            'agentType': 'ai'
        })
        
        # Step 1: Count zero-dimension elements  
        result1 = await client.js.execute("""
            const allElements = document.querySelectorAll('*');
            let zeroCount = 0;
            const tags = {};
            
            for (let i = 0; i < allElements.length; i++) {
                const el = allElements[i];
                if (el.offsetWidth === 0 || el.offsetHeight === 0) {
                    zeroCount++;
                    const tag = el.tagName;
                    tags[tag] = (tags[tag] || 0) + 1;
                }
            }
            
            console.log('Zero elements by tag:', JSON.stringify(tags));
            return 'Found ' + zeroCount + ' zero elements';
        """)
        print(f"Step 1 - Zero elements: {result1}")
        
        # Step 2: Test basic html2canvas on version element
        result2 = await client.js.execute("""
            (async function() {
                const versionElement = document.querySelector('.version-badge');
                if (!versionElement) return 'NO_VERSION_ELEMENT';
                
                console.log('Version element size:', versionElement.offsetWidth + 'x' + versionElement.offsetHeight);
                
                try {
                    const canvas = await html2canvas(versionElement, { scale: 1 });
                    console.log('Basic html2canvas SUCCESS:', canvas.width + 'x' + canvas.height);
                    return 'BASIC_SUCCESS';
                } catch (error) {
                    console.log('Basic html2canvas FAILED:', error.message);
                    return 'BASIC_FAILED: ' + error.message;
                }
            })();
        """)
        print(f"Step 2 - Basic html2canvas: {result2}")
        
        # Step 3: Test exact continuum API config with ignoreElements fix
        result3 = await client.js.execute("""
            (async function() {
                const versionElement = document.querySelector('.version-badge');
                if (!versionElement) return 'NO_VERSION_ELEMENT';
                
                console.log('Testing continuum API config with ignoreElements fix...');
                
                try {
                    const canvas = await html2canvas(versionElement, {
                        allowTaint: true,
                        useCORS: true,
                        scale: 1.0,
                        backgroundColor: '#1a1a1a',
                        ignoreElements: function(element) {
                            const isZero = element.offsetWidth === 0 || element.offsetHeight === 0;
                            if (isZero) {
                                console.log('Ignoring zero element:', element.tagName);
                            }
                            return isZero;
                        }
                    });
                    
                    console.log('✅ FIXED html2canvas SUCCESS:', canvas.width + 'x' + canvas.height);
                    
                    // Test sending via WebSocket
                    const dataURL = canvas.toDataURL('image/png');
                    const message = {
                        type: 'screenshot_data',
                        dataURL: dataURL,
                        filename: 'debug_fixed_version.png',
                        timestamp: Date.now(),
                        dimensions: { width: canvas.width, height: canvas.height }
                    };
                    
                    window.ws.send(JSON.stringify(message));
                    console.log('✅ Screenshot sent via WebSocket');
                    
                    return 'FIXED_SUCCESS';
                } catch (error) {
                    console.log('❌ FIXED html2canvas FAILED:', error.message);
                    return 'FIXED_FAILED: ' + error.message;
                }
            })();
        """)
        
        result = { 'step1': result1, 'step2': result2, 'step3': result3 }
        
        print(f"CreatePattern debug result: {result}")
        
        import time
        time.sleep(2)
        
        # Check if the working version screenshot was created
        screenshot_dir = Path('.continuum/screenshots')
        if screenshot_dir.exists():
            files = list(screenshot_dir.glob('debug_version_working.png'))
            print(f"Working version files: {len(files)}")
            if files:
                for f in files:
                    print(f"  ✅ {f.name} ({f.stat().st_size} bytes)")
                    import subprocess
                    subprocess.run(['open', str(f)], check=False)
            else:
                print("❌ No working version file created - WebSocket processing still broken")
        else:
            print("❌ Screenshot directory not found")

asyncio.run(debug_createpattern())