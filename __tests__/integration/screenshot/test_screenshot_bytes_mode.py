#!/usr/bin/env python3
"""
Test Screenshot Bytes Mode
Test the Python client screenshot functionality that returns bytes
"""

import asyncio
import sys
from pathlib import Path
import base64

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def test_screenshot_bytes_mode():
    print("🧪 TESTING SCREENSHOT BYTES MODE")
    print("This test captures screenshot bytes and saves them locally")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'test-screenshot-bytes',
            'agentName': 'Test Screenshot Bytes Mode',
            'agentType': 'ai'
        })
        
        # Test 1: Capture version badge and get bytes back
        print("\n--- TEST 1: Capture version badge bytes ---")
        result = await client.js.execute("""
            console.log('📸 Capturing version badge for bytes test...');
            
            const versionElement = document.querySelector('.version-badge');
            if (!versionElement) {
                return {success: false, error: 'Version badge not found'};
            }
            
            console.log('📐 Version element:', versionElement.offsetWidth + 'x' + versionElement.offsetHeight);
            
            // Use consolidated ScreenshotUtils if available
            if (typeof window.ScreenshotUtils !== 'undefined') {
                try {
                    return window.ScreenshotUtils.takeScreenshot(versionElement, {
                        scale: 1.0,
                        source: 'bytes_test'
                    }).then(function(canvas) {
                        const dataURL = canvas.toDataURL('image/png');
                        
                        return {
                            success: true,
                            dataURL: dataURL,
                            width: canvas.width,
                            height: canvas.height,
                            method: 'ScreenshotUtils'
                        };
                    }).catch(function(error) {
                        console.error('❌ ScreenshotUtils failed:', error.message);
                        return {success: false, error: 'ScreenshotUtils failed: ' + error.message};
                    });
                } catch (error) {
                    console.error('❌ ScreenshotUtils exception:', error.message);
                    return {success: false, error: 'ScreenshotUtils exception: ' + error.message};
                }
            } else {
                return {success: false, error: 'ScreenshotUtils not available'};
            }
        """)
        
        if result.get('success'):
            data = result.get('result')
            if isinstance(data, str):
                import json
                data = json.loads(data)
            
            if data.get('success'):
                print(f"✅ Screenshot captured: {data['width']}x{data['height']} via {data['method']}")
                
                # Extract base64 data and save as PNG
                data_url = data['dataURL']
                if data_url.startswith('data:image/png;base64,'):
                    base64_data = data_url.split(',')[1]
                    image_bytes = base64.b64decode(base64_data)
                    
                    # Save to local file for verification
                    output_file = Path('python-client/test_screenshots/bytes_mode_version.png')
                    output_file.parent.mkdir(exist_ok=True)
                    
                    with open(output_file, 'wb') as f:
                        f.write(image_bytes)
                    
                    file_size = len(image_bytes)
                    print(f"✅ Saved bytes to {output_file} ({file_size} bytes)")
                    
                    # Verify it's a valid PNG
                    if image_bytes.startswith(b'\x89PNG'):
                        print("✅ Valid PNG header detected")
                        return True
                    else:
                        print("❌ Invalid PNG header")
                        return False
                else:
                    print("❌ Invalid data URL format")
                    return False
            else:
                print(f"❌ Screenshot failed: {data.get('error')}")
                return False
        else:
            print(f"❌ JS execution failed: {result.get('error')}")
            return False

async def test_file_mode_comparison():
    print("\n🧪 TESTING FILE MODE FOR COMPARISON")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'test-file-mode',
            'agentName': 'Test File Mode',
            'agentType': 'ai'
        })
        
        # Test using the consolidated continuum.command.screenshot() API
        result = await client.js.execute("""
            console.log('📸 Testing unified continuum.command.screenshot()...');
            
            if (typeof window.continuum === 'undefined' || !window.continuum.command) {
                return {success: false, error: 'continuum.command not available'};
            }
            
            try {
                const promise = window.continuum.command.screenshot({
                    selector: '.version-badge',
                    name_prefix: 'unit_test_file_mode',
                    scale: 1.0
                });
                
                console.log('📸 Screenshot promise created');
                return {success: true, message: 'Screenshot command sent'};
                
            } catch (error) {
                console.error('❌ continuum.command.screenshot failed:', error.message);
                return {success: false, error: error.message};
            }
        """)
        
        if result.get('success'):
            data = result.get('result')
            if isinstance(data, str):
                import json
                data = json.loads(data)
            
            if data.get('success'):
                print("✅ File mode screenshot command sent successfully")
                
                # Wait a moment for file to be created
                await asyncio.sleep(3)
                
                # Check if file was created
                screenshot_dir = Path('.continuum/screenshots')
                if screenshot_dir.exists():
                    files = list(screenshot_dir.glob('unit_test_file_mode_*.png'))
                    if files:
                        latest_file = max(files, key=lambda f: f.stat().st_mtime)
                        file_size = latest_file.stat().st_size
                        print(f"✅ File mode created: {latest_file.name} ({file_size} bytes)")
                        return True
                    else:
                        print("❌ No file mode files found")
                        return False
                else:
                    print("❌ Screenshot directory not found")
                    return False
            else:
                print(f"❌ File mode failed: {data.get('error')}")
                return False
        else:
            print(f"❌ File mode JS execution failed: {result.get('error')}")
            return False

async def main():
    print("🚀 TESTING BOTH SCREENSHOT MODES")
    
    # Test bytes mode (for unit tests)
    bytes_success = await test_screenshot_bytes_mode()
    
    # Test file mode (for production use)
    file_success = await test_file_mode_comparison()
    
    print(f"\n📊 RESULTS:")
    print(f"  Bytes mode: {'✅ PASS' if bytes_success else '❌ FAIL'}")
    print(f"  File mode:  {'✅ PASS' if file_success else '❌ FAIL'}")
    
    if bytes_success and file_success:
        print("\n🎉 Both screenshot modes working correctly!")
        print("  - Bytes mode: Perfect for unit tests and programmatic use")
        print("  - File mode: Perfect for production screenshot saving")
        return True
    else:
        print("\n💥 One or both modes failed!")
        return False

if __name__ == "__main__":
    success = asyncio.run(main())
    sys.exit(0 if success else 1)