#!/usr/bin/env python3
"""
Simple screenshot test to verify the system works
"""

import asyncio
import json
import base64
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_simple_screenshot():
    """Test basic screenshot functionality"""
    
    load_continuum_config()
    
    print("📸 Testing simple screenshot capture...")
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'simple-screenshot-test',
            'agentName': 'Simple Screenshot Test',
            'agentType': 'ai'
        })
        
        # Simple screenshot test
        screenshot_js = """
        return JSON.stringify({
            message: 'Screenshot system ready',
            timestamp: Date.now(),
            success: true
        });
        """
        
        try:
            result_json = await client.js.get_value(screenshot_js)
            result = json.loads(result_json)
            print(f"✅ Basic test passed: {result}")
        except Exception as e:
            print(f"❌ Basic test failed: {e}")
            return
        
        # Test html2canvas loading
        html2canvas_test = """
        return new Promise((resolve) => {
            console.log('📸 Testing html2canvas loading...');
            
            if (typeof html2canvas !== 'undefined') {
                resolve(JSON.stringify({
                    loaded: true,
                    message: 'html2canvas already available'
                }));
            } else {
                const script = document.createElement('script');
                script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                script.onload = () => {
                    console.log('✅ html2canvas loaded successfully');
                    resolve(JSON.stringify({
                        loaded: true,
                        message: 'html2canvas loaded successfully'
                    }));
                };
                script.onerror = () => {
                    resolve(JSON.stringify({
                        loaded: false,
                        error: 'Failed to load html2canvas'
                    }));
                };
                document.head.appendChild(script);
            }
        });
        """
        
        try:
            result_json = await client.js.get_value(html2canvas_test, timeout=15)
            result = json.loads(result_json)
            print(f"✅ html2canvas test: {result}")
            
            if not result['loaded']:
                print("❌ html2canvas failed to load, cannot continue")
                return
                
        except Exception as e:
            print(f"❌ html2canvas test failed: {e}")
            return
        
        # Simple screenshot capture
        simple_screenshot = """
        return new Promise((resolve, reject) => {
            console.log('📸 Starting simple screenshot...');
            
            // Create a small test element
            const testEl = document.createElement('div');
            testEl.style.width = '100px';
            testEl.style.height = '50px';
            testEl.style.backgroundColor = '#FF5722';
            testEl.style.color = 'white';
            testEl.style.textAlign = 'center';
            testEl.style.lineHeight = '50px';
            testEl.style.position = 'fixed';
            testEl.style.top = '10px';
            testEl.style.left = '10px';
            testEl.style.zIndex = '9999';
            testEl.textContent = 'TEST';
            document.body.appendChild(testEl);
            
            // Wait for element to render
            setTimeout(() => {
                html2canvas(testEl, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 1
                }).then(canvas => {
                    // Clean up
                    document.body.removeChild(testEl);
                    
                    const dataURL = canvas.toDataURL('image/png');
                    const result = {
                        success: true,
                        width: canvas.width,
                        height: canvas.height,
                        dataURL: dataURL,
                        dataSize: dataURL.length
                    };
                    
                    console.log('✅ Screenshot captured:', result.width + 'x' + result.height);
                    resolve(JSON.stringify(result));
                    
                }).catch(error => {
                    if (document.body.contains(testEl)) {
                        document.body.removeChild(testEl);
                    }
                    reject(error.message);
                });
            }, 200);
        });
        """
        
        try:
            result_json = await client.js.get_value(simple_screenshot, timeout=15)
            result = json.loads(result_json)
            
            print(f"✅ Screenshot captured!")
            print(f"   Size: {result['width']}x{result['height']}")
            print(f"   Data size: {result['dataSize']} characters")
            print(f"   Format: {'PNG' if result['dataURL'].startswith('data:image/png') else 'Unknown'}")
            
            # Verify base64 data
            if result['dataURL'].startswith('data:image/png;base64,'):
                base64_data = result['dataURL'].split(',')[1]
                try:
                    decoded = base64.b64decode(base64_data)
                    print(f"   Base64 valid: {len(decoded)} bytes decoded")
                except Exception as e:
                    print(f"   Base64 invalid: {e}")
            
            print("🎉 Screenshot system is working!")
            
        except Exception as e:
            print(f"❌ Screenshot capture failed: {e}")

if __name__ == "__main__":
    asyncio.run(test_simple_screenshot())