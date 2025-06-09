#!/usr/bin/env python3
"""
Debug canvas elements directly in browser to find the 0x0 culprits
"""

import asyncio
import websockets
import json
import base64

async def debug_canvas_elements():
    print("🔍 DEBUGGING CANVAS ELEMENTS IN BROWSER")
    print("=" * 50)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to Continuum")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # JavaScript to inspect all canvas elements
            debug_js = """
            console.log("🔍 CANVAS ELEMENT DEBUG STARTING...");
            
            // Find all canvas elements and their properties
            const canvases = document.querySelectorAll('canvas');
            console.log("Found " + canvases.length + " canvas elements");
            
            const canvasData = [];
            
            canvases.forEach((canvas, index) => {
                const data = {
                    index: index,
                    tagName: canvas.tagName,
                    width: canvas.width,
                    height: canvas.height,
                    offsetWidth: canvas.offsetWidth,
                    offsetHeight: canvas.offsetHeight,
                    clientWidth: canvas.clientWidth,
                    clientHeight: canvas.clientHeight,
                    style_display: canvas.style.display,
                    style_visibility: canvas.style.visibility,
                    className: canvas.className,
                    id: canvas.id,
                    parentElement: canvas.parentElement ? canvas.parentElement.tagName : 'none',
                    isZeroDimension: (canvas.width === 0 || canvas.height === 0),
                    isZeroOffset: (canvas.offsetWidth === 0 || canvas.offsetHeight === 0),
                    isHidden: (canvas.style.display === 'none' || canvas.style.visibility === 'hidden')
                };
                
                canvasData.push(data);
                
                console.log("Canvas " + index + ":", data);
                
                if (data.isZeroDimension) {
                    console.warn("⚠️ ZERO DIMENSION CANVAS FOUND:", data);
                }
            });
            
            // Also check for SVG elements that might cause issues
            const svgs = document.querySelectorAll('svg');
            console.log("Found " + svgs.length + " SVG elements");
            
            const svgData = [];
            svgs.forEach((svg, index) => {
                const data = {
                    index: index,
                    tagName: svg.tagName,
                    width: svg.getAttribute('width'),
                    height: svg.getAttribute('height'),
                    offsetWidth: svg.offsetWidth,
                    offsetHeight: svg.offsetHeight,
                    viewBox: svg.getAttribute('viewBox'),
                    isZeroAttribute: (svg.getAttribute('width') === '0' || svg.getAttribute('height') === '0'),
                    isZeroOffset: (svg.offsetWidth === 0 || svg.offsetHeight === 0)
                };
                
                svgData.push(data);
                
                if (data.isZeroAttribute || data.isZeroOffset) {
                    console.warn("⚠️ PROBLEMATIC SVG FOUND:", data);
                }
            });
            
            // Test html2canvas availability and try a minimal test
            let html2canvasTest = "not_available";
            if (typeof html2canvas !== 'undefined') {
                console.log("html2canvas is available");
                html2canvasTest = "available";
                
                // Try to identify which element is causing the createPattern error
                console.log("🧪 Testing html2canvas on small elements...");
                
                try {
                    // Create a simple test div
                    const testDiv = document.createElement('div');
                    testDiv.style.width = '100px';
                    testDiv.style.height = '100px';
                    testDiv.style.backgroundColor = 'red';
                    testDiv.innerHTML = 'TEST';
                    document.body.appendChild(testDiv);
                    
                    html2canvas(testDiv, {
                        allowTaint: true,
                        scale: 0.1
                    }).then(function(canvas) {
                        console.log("✅ Test div screenshot SUCCESS");
                        document.body.removeChild(testDiv);
                    }).catch(function(error) {
                        console.error("❌ Test div screenshot FAILED:", error.message);
                        document.body.removeChild(testDiv);
                    });
                    
                } catch (e) {
                    console.error("❌ Exception in html2canvas test:", e.message);
                }
            }
            
            return {
                canvasCount: canvases.length,
                canvasData: canvasData,
                svgCount: svgs.length,
                svgData: svgData,
                html2canvasAvailable: html2canvasTest,
                zeroDimensionCanvases: canvasData.filter(c => c.isZeroDimension).length,
                zeroOffsetCanvases: canvasData.filter(c => c.isZeroOffset).length,
                problematicSvgs: svgData.filter(s => s.isZeroAttribute || s.isZeroOffset).length
            };
            """
            
            encoded = base64.b64encode(debug_js.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded}'
            }
            
            print("📤 Sending canvas debug command...")
            await websocket.send(json.dumps(command))
            
            # Wait for detailed results
            for attempt in range(5):
                try:
                    response = await asyncio.wait_for(websocket.recv(), timeout=4)
                    data = json.loads(response)
                    
                    if data.get('type') == 'result':
                        try:
                            # Navigate the nested structure
                            result_data = data.get('data', {})
                            inner_result = result_data.get('result', {})
                            browser_result = inner_result.get('result', {})
                            browser_response = browser_result.get('browserResponse', {})
                            console_output = browser_response.get('output', [])
                            return_value = browser_response.get('result')
                            
                            print(f"\n📋 CANVAS DEBUG RESULTS:")
                            print("=" * 40)
                            
                            if return_value:
                                try:
                                    if isinstance(return_value, str):
                                        result_obj = json.loads(return_value)
                                    else:
                                        result_obj = return_value
                                    
                                    print(f"Canvas elements found: {result_obj.get('canvasCount', 0)}")
                                    print(f"SVG elements found: {result_obj.get('svgCount', 0)}")
                                    print(f"html2canvas available: {result_obj.get('html2canvasAvailable')}")
                                    print(f"Zero dimension canvases: {result_obj.get('zeroDimensionCanvases', 0)}")
                                    print(f"Zero offset canvases: {result_obj.get('zeroOffsetCanvases', 0)}")
                                    print(f"Problematic SVGs: {result_obj.get('problematicSvgs', 0)}")
                                    
                                    # Show detailed canvas data
                                    canvas_data = result_obj.get('canvasData', [])
                                    if canvas_data:
                                        print(f"\n📋 DETAILED CANVAS ANALYSIS:")
                                        for canvas in canvas_data:
                                            if canvas.get('isZeroDimension') or canvas.get('isZeroOffset'):
                                                print(f"🚨 PROBLEMATIC CANVAS #{canvas.get('index')}:")
                                                print(f"   Dimensions: {canvas.get('width')}x{canvas.get('height')}")
                                                print(f"   Offset: {canvas.get('offsetWidth')}x{canvas.get('offsetHeight')}")
                                                print(f"   Class: {canvas.get('className')}")
                                                print(f"   ID: {canvas.get('id')}")
                                                print(f"   Parent: {canvas.get('parentElement')}")
                                                print(f"   Hidden: {canvas.get('isHidden')}")
                                    
                                except Exception as e:
                                    print(f"❌ Error parsing result object: {e}")
                                    print(f"Raw result: {return_value}")
                            
                            print(f"\n📋 CONSOLE OUTPUT FROM DEBUG:")
                            print("=" * 40)
                            
                            for msg in console_output:
                                level = msg.get('level', 'unknown')
                                message = msg.get('message', '')
                                
                                if level == 'error':
                                    print(f"🚨 ERROR: {message}")
                                elif level == 'warn':
                                    print(f"⚠️  WARN: {message}")
                                else:
                                    print(f"📝 {level.upper()}: {message}")
                            
                            return True
                            
                        except Exception as e:
                            print(f"❌ Error parsing debug result: {e}")
                            return False
                            
                    elif data.get('type') == 'working':
                        print("⏳ Processing debug...")
                        continue
                        
                except asyncio.TimeoutError:
                    print(f"⏰ Timeout {attempt + 1}/5")
                    continue
            
            print("❌ No debug result received")
            return False
            
    except Exception as e:
        print(f"❌ Debug error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(debug_canvas_elements())
    print(f"\n🎯 CANVAS DEBUG: {'COMPLETED' if result else 'FAILED'}")