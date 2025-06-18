#!/usr/bin/env python3
"""
Debug CreatePattern Error for Whole Screen
Investigate what zero-dimension elements are causing the createPattern error
"""

import asyncio
import sys
from pathlib import Path
import json

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def debug_createpattern():
    print("🔍 DEBUGGING CREATEPATTERN ERROR FOR WHOLE SCREEN")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'debug-createpattern-whole',
            'agentName': 'Debug CreatePattern Whole Screen',
            'agentType': 'ai'
        })
        
        # Comprehensive analysis of zero-dimension elements
        result = await client.js.execute("""
            console.log('🔍 Analyzing zero-dimension elements in document.body...');
            
            function analyzeElement(element, path = '') {
                const info = {
                    tagName: element.tagName,
                    path: path,
                    offsetWidth: element.offsetWidth,
                    offsetHeight: element.offsetHeight,
                    clientWidth: element.clientWidth,
                    clientHeight: element.clientHeight,
                    scrollWidth: element.scrollWidth,
                    scrollHeight: element.scrollHeight,
                    className: element.className,
                    id: element.id
                };
                
                // Additional canvas-specific checks
                if (element.tagName === 'CANVAS') {
                    info.canvasWidth = element.width;
                    info.canvasHeight = element.height;
                    info.canvasStyle = {
                        width: element.style.width,
                        height: element.style.height
                    };
                }
                
                // CSS properties
                const computed = window.getComputedStyle(element);
                info.computed = {
                    display: computed.display,
                    visibility: computed.visibility,
                    width: computed.width,
                    height: computed.height,
                    position: computed.position
                };
                
                return info;
            }
            
            // Find all zero-dimension elements
            const allElements = document.body.querySelectorAll('*');
            const zeroElements = [];
            const canvasElements = [];
            
            for (let i = 0; i < allElements.length; i++) {
                const element = allElements[i];
                const isZero = element.offsetWidth === 0 || element.offsetHeight === 0;
                
                if (isZero) {
                    zeroElements.push(analyzeElement(element, `element[${i}]`));
                }
                
                if (element.tagName === 'CANVAS') {
                    canvasElements.push(analyzeElement(element, `canvas[${canvasElements.length}]`));
                }
            }
            
            // Also analyze document.body itself
            const bodyInfo = analyzeElement(document.body, 'body');
            
            console.log('📊 Found', zeroElements.length, 'zero-dimension elements');
            console.log('📊 Found', canvasElements.length, 'canvas elements');
            
            return {
                success: true,
                bodyInfo: bodyInfo,
                zeroElements: zeroElements,
                canvasElements: canvasElements,
                totalElements: allElements.length,
                zeroCount: zeroElements.length
            };
        """)
        
        if result['success']:
            data = json.loads(result['result'])
            
            print(f"\n📊 Element Analysis:")
            print(f"  Total elements: {data['totalElements']}")
            print(f"  Zero-dimension elements: {data['zeroCount']}")
            print(f"  Canvas elements: {len(data['canvasElements'])}")
            
            # Show body info
            body_info = data['bodyInfo']
            print(f"\n📐 Body element:")
            print(f"  Offset: {body_info['offsetWidth']}x{body_info['offsetHeight']}")
            print(f"  Client: {body_info['clientWidth']}x{body_info['clientHeight']}")
            print(f"  Scroll: {body_info['scrollWidth']}x{body_info['scrollHeight']}")
            
            # Show problematic zero elements
            zero_elements = data['zeroElements']
            if zero_elements:
                print(f"\n🚫 Zero-dimension elements ({len(zero_elements)}):")
                
                # Group by tag name for easier analysis
                by_tag = {}
                for elem in zero_elements:
                    tag = elem['tagName']
                    if tag not in by_tag:
                        by_tag[tag] = []
                    by_tag[tag].append(elem)
                
                for tag, elements in sorted(by_tag.items()):
                    print(f"  📋 {tag}: {len(elements)} elements")
                    # Show first few examples
                    for i, elem in enumerate(elements[:3]):
                        display = elem['computed']['display']
                        visibility = elem['computed']['visibility']
                        print(f"    {i+1}. {elem['className'][:30] if elem['className'] else elem['id'][:30] if elem['id'] else 'no-class-id'} (display:{display}, vis:{visibility})")
            
            # Show canvas elements specifically
            canvas_elements = data['canvasElements']
            if canvas_elements:
                print(f"\n🎨 Canvas elements ({len(canvas_elements)}):")
                for i, canvas in enumerate(canvas_elements):
                    print(f"  {i+1}. Canvas {canvas['canvasWidth']}x{canvas['canvasHeight']} (offset: {canvas['offsetWidth']}x{canvas['offsetHeight']})")
                    print(f"     Display: {canvas['computed']['display']}, Style: {canvas['canvasStyle']}")
        
        # Now test with the enhanced ignoreElements function manually
        print(f"\n🧪 Testing enhanced ignoreElements function...")
        
        test_result = await client.js.execute("""
            console.log('🧪 Testing screenshot with enhanced ignoreElements...');
            
            // Try a smaller element first to see if ignoreElements is working
            const testDiv = document.createElement('div');
            testDiv.style.width = '100px';
            testDiv.style.height = '100px';
            testDiv.style.background = 'red';
            testDiv.style.position = 'fixed';
            testDiv.style.top = '10px';
            testDiv.style.left = '10px';
            testDiv.style.zIndex = '9999';
            testDiv.innerHTML = 'TEST DIV';
            
            document.body.appendChild(testDiv);
            
            return window.ScreenshotUtils.takeScreenshot(testDiv, {
                scale: 1.0,
                source: 'test_enhanced_ignore'
            }).then(function(canvas) {
                // Clean up
                document.body.removeChild(testDiv);
                
                return {
                    success: true,
                    testResult: 'test_div_success',
                    width: canvas.width,
                    height: canvas.height
                };
            }).catch(function(error) {
                // Clean up on error
                if (document.body.contains(testDiv)) {
                    document.body.removeChild(testDiv);
                }
                
                return {
                    success: false,
                    error: error.message,
                    testResult: 'test_div_failed'
                };
            });
        """)
        
        if test_result['success']:
            test_data = json.loads(test_result['result'])
            if test_data['success']:
                print(f"✅ Test div capture worked: {test_data['width']}x{test_data['height']}")
                print("  Enhanced ignoreElements function is working")
                
                # The enhanced function works for small elements, so the issue is specifically with document.body
                print("\n🔍 The issue is specifically with document.body containing problematic elements")
                print("   Enhanced ignoreElements is working, but document.body has too many complex elements")
                
            else:
                print(f"❌ Even test div failed: {test_data.get('error')}")
                print("  Enhanced ignoreElements function might have issues")
        
        print(f"\n💡 DIAGNOSIS:")
        print(f"  - Enhanced ignoreElements is filtering more elements")
        print(f"  - document.body contains {data['zeroCount']} zero-dimension elements")
        print(f"  - Canvas elements: {len(data['canvasElements'])}")
        print(f"  - Issue likely: html2canvas still finds problematic elements in document.body")
        print(f"  - Solution: Use a wrapper div or more specific selector instead of document.body")

if __name__ == "__main__":
    asyncio.run(debug_createpattern())