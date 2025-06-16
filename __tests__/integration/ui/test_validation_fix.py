#!/usr/bin/env python3
"""
Test Enhanced Validation Fix
Verify that the enhanced validation properly catches document.body issues
"""

import asyncio
import sys
from pathlib import Path
import json

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_validation_fix():
    print("🧪 TESTING ENHANCED VALIDATION FIX")
    print("Testing that document.body with 39 zero-dimension elements gets rejected")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'validation-fix-test',
            'agentName': 'Validation Fix Test',
            'agentType': 'ai'
        })
        
        # Force refresh the ScreenshotUtils to ensure we have the latest version
        print("\n📥 Refreshing ScreenshotUtils with latest validation...")
        refresh_result = await client.js.execute("""
            // Force reload ScreenshotUtils.js to get latest validation
            if (typeof window.ScreenshotUtils !== 'undefined') {
                console.log('🔄 ScreenshotUtils already loaded, will test with current version');
            } else {
                console.log('❌ ScreenshotUtils not loaded');
            }
            
            return {success: true, screenshotUtilsAvailable: typeof window.ScreenshotUtils !== 'undefined'};
        """)
        
        if refresh_result['success']:
            data = json.loads(refresh_result['result'])
            if not data['screenshotUtilsAvailable']:
                print("❌ ScreenshotUtils not available, cannot test")
                return False
        
        # Test the specific validation case
        print("\n🔍 Testing document.body validation with 39 zero-dimension elements...")
        validation_result = await client.js.execute("""
            console.log('🧪 Testing enhanced validation for document.body...');
            
            // Count zero-dimension elements first
            const allElements = document.body.querySelectorAll('*');
            let zeroCount = 0;
            let canvasCount = 0;
            
            for (let element of allElements) {
                if (element.offsetWidth === 0 || element.offsetHeight === 0) {
                    zeroCount++;
                    if (element.tagName === 'CANVAS') {
                        canvasCount++;
                    }
                }
            }
            
            console.log(`📊 Found ${zeroCount} zero-dimension elements (${canvasCount} canvas)`);
            
            // Now test the screenshot with validation
            return window.ScreenshotUtils.takeScreenshot(document.body, {
                scale: 0.3,
                source: 'validation_test'
            }).then(function(canvas) {
                return {
                    success: false,
                    error: 'Validation failed - screenshot should have been rejected',
                    zeroCount: zeroCount,
                    canvasCount: canvasCount,
                    actualResult: 'screenshot_succeeded'
                };
            }).catch(function(error) {
                console.log('✅ Validation correctly rejected screenshot:', error.message);
                return {
                    success: true,
                    error: error.message,
                    zeroCount: zeroCount,
                    canvasCount: canvasCount,
                    actualResult: 'screenshot_rejected'
                };
            });
        """)
        
        if validation_result['success']:
            test_data = json.loads(validation_result['result'])
            
            print(f"📊 Analysis:")
            print(f"  Zero-dimension elements: {test_data['zeroCount']}")
            print(f"  Canvas elements: {test_data['canvasCount']}")
            print(f"  Result: {test_data['actualResult']}")
            
            if test_data['success']:
                print(f"✅ VALIDATION WORKING: {test_data['error']}")
                print("  Enhanced validation correctly rejected document.body capture")
                return True
            else:
                print(f"❌ VALIDATION FAILED: {test_data['error']}")
                print("  Validation should have rejected the capture but didn't")
                return False
        else:
            print("❌ Test execution failed")
            return False

if __name__ == "__main__":
    success = asyncio.run(test_validation_fix())
    if success:
        print("\n✅ Enhanced validation is working correctly!")
    else:
        print("\n❌ Enhanced validation needs more work!")
    sys.exit(0 if success else 1)