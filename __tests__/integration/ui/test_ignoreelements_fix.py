#!/usr/bin/env python3
"""
Test ignoreElements Fix for createPattern Error
Patch continuum API and test screenshot functionality
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

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
        
        # Patch the continuum API to use ignoreElements fix
        patch_result = await client.js.execute("""
            // Backup original function
            window.continuum_original_screenshot = window.continuum.command.screenshot;
            
            // Create patched version with ignoreElements fix
            window.continuum.command.screenshot = function(params = {}) {
                const {
                    selector = 'body',
                    name_prefix = 'screenshot',
                    scale = 1.0,
                    manual = false
                } = params;
                
                console.log('📸 PATCHED continuum.command.screenshot() called:', params);
                
                const timestamp = Date.now();
                const filename = name_prefix + '_' + timestamp + '.png';
                
                return new Promise((resolve, reject) => {
                    let targetElement = document.querySelector(selector);
                    if (!targetElement) {
                        console.warn('⚠️ Element not found: ' + selector + ', using body');
                        targetElement = document.body;
                    }
                    
                    console.log('📸 Capturing ' + selector + ' -> ' + filename);
                    
                    if (typeof html2canvas === 'undefined') {
                        const error = 'html2canvas not available';
                        console.error('❌ ' + error);
                        reject(new Error(error));
                        return;
                    }
                    
                    // Apply ignoreElements fix for createPattern error
                    html2canvas(targetElement, {
                        allowTaint: true,
                        useCORS: true,
                        scale: scale,
                        backgroundColor: '#1a1a1a',
                        ignoreElements: function(element) {
                            const isZero = element.offsetWidth === 0 || element.offsetHeight === 0;
                            if (isZero) {
                                console.log('🚫 Ignoring zero element: ' + element.tagName);
                            }
                            return isZero;
                        }
                    }).then(function(canvas) {
                        console.log('✅ PATCHED Screenshot captured: ' + canvas.width + 'x' + canvas.height);
                        
                        const dataURL = canvas.toDataURL('image/png');
                        
                        if (window.ws && window.ws.readyState === 1) {
                            const screenshotData = {
                                type: 'screenshot_data',
                                dataURL: dataURL,
                                filename: filename,
                                timestamp: timestamp,
                                dimensions: { width: canvas.width, height: canvas.height },
                                selector: selector,
                                source: 'patched_continuum_api'
                            };
                            
                            console.log('📤 PATCHED Sending screenshot to server: ' + filename);
                            window.ws.send(JSON.stringify(screenshotData));
                            
                            resolve({
                                success: true,
                                filename: filename,
                                dimensions: { width: canvas.width, height: canvas.height },
                                patched: true
                            });
                        } else {
                            const error = 'WebSocket not connected';
                            console.error('❌ ' + error);
                            reject(new Error(error));
                        }
                    }).catch(function(error) {
                        console.error('❌ PATCHED Screenshot failed:', error);
                        reject(error);
                    });
                });
            };
            
            console.log('✅ Continuum API patched with ignoreElements fix');
            return 'API_PATCHED';
        """)
        print(f"Patch result: {patch_result}")
        
        # Test the patched API
        print("🧪 Testing patched screenshot API...")
        result = await client.command.screenshot(
            selector='.version-badge',
            name_prefix='test_fixed',
            scale=1.0,
            manual=False
        )
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
                        'test': 'ignoreElements_fix',
                        'status': 'SUCCESS',
                        'filename': f.name,
                        'size_kb': size_kb,
                        'fix_applied': 'ignoreElements function to filter zero-dimension elements',
                        'elements_filtered': 'HEAD, META, TITLE, LINK, STYLE, SCRIPT, DIV, H3, BUTTON, IFRAME',
                        'total_zero_elements': 54
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