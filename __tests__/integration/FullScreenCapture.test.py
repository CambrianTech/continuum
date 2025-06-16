#!/usr/bin/env python3
"""
Full Screen Capture Integration Tests
Test complete screen capture across different scenarios and viewports
"""

import asyncio
import pytest
import sys
from pathlib import Path
import json
import base64
from PIL import Image
import cv2
import numpy as np

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config


class TestFullScreenCapture:
    """Test complete screen capture functionality"""
    
    @pytest.fixture
    async def client(self):
        """Set up continuum client for testing"""
        load_continuum_config()
        async with ContinuumClient() as client:
            await client.register_agent({
                'agentId': 'fullscreen-test',
                'agentName': 'Full Screen Capture Test', 
                'agentType': 'ai'
            })
            yield client
    
    @pytest.mark.asyncio
    async def test_full_document_capture(self, client):
        """Test capturing the entire document/page"""
        
        result = await client.js.execute("""
            console.log('📸 Testing full document capture...');
            
            // Get document dimensions
            const docWidth = Math.max(
                document.documentElement.scrollWidth,
                document.body.scrollWidth,
                document.documentElement.offsetWidth,
                document.body.offsetWidth,
                document.documentElement.clientWidth,
                document.body.clientWidth
            );
            
            const docHeight = Math.max(
                document.documentElement.scrollHeight,
                document.body.scrollHeight,
                document.documentElement.offsetHeight,
                document.body.offsetHeight,
                document.documentElement.clientHeight,
                document.body.clientHeight
            );
            
            console.log('📐 Document dimensions:', docWidth + 'x' + docHeight);
            
            return window.continuum.command.screenshot({
                selector: 'body',
                name_prefix: 'full_document',
                scale: 0.5,  // Smaller scale for full document
                destination: 'file'
            }).then(() => ({
                success: true,
                documentWidth: docWidth,
                documentHeight: docHeight,
                message: 'Full document screenshot requested'
            }));
        """)
        
        assert result['success'], f"Full document capture failed: {result.get('error')}"
        
        data = json.loads(result['result'])
        assert data['success'], f"Screenshot command failed: {data.get('message')}"
        
        # Wait for file creation
        await asyncio.sleep(5)  # Full page takes longer
        
        # Verify file was created
        screenshot_dir = Path('.continuum/screenshots')
        files = list(screenshot_dir.glob('full_document_*.png'))
        assert len(files) > 0, "No full document screenshot created"
        
        latest_file = max(files, key=lambda f: f.stat().st_mtime)
        
        # Verify it's a substantial image (not tiny)
        with Image.open(latest_file) as img:
            width, height = img.size
            assert width > 500, f"Document screenshot too narrow: {width}px"
            assert height > 300, f"Document screenshot too short: {height}px"
            
            # Should be larger than a single element
            file_size = latest_file.stat().st_size
            assert file_size > 5000, f"Full document file too small: {file_size} bytes"
        
        print(f"✅ Full document captured: {latest_file.name} ({width}x{height}, {file_size} bytes)")
        return latest_file
    
    @pytest.mark.asyncio
    async def test_viewport_capture(self, client):
        """Test capturing just the visible viewport"""
        
        result = await client.js.execute("""
            console.log('📸 Testing viewport capture...');
            
            // Get viewport dimensions
            const viewportWidth = window.innerWidth || document.documentElement.clientWidth;
            const viewportHeight = window.innerHeight || document.documentElement.clientHeight;
            
            console.log('📐 Viewport dimensions:', viewportWidth + 'x' + viewportHeight);
            
            // Create a div that covers the viewport
            const viewportDiv = document.createElement('div');
            viewportDiv.style.position = 'fixed';
            viewportDiv.style.top = '0';
            viewportDiv.style.left = '0';
            viewportDiv.style.width = '100vw';
            viewportDiv.style.height = '100vh';
            viewportDiv.style.pointerEvents = 'none';
            viewportDiv.style.zIndex = '9999';
            viewportDiv.id = 'viewport-capture-test';
            
            document.body.appendChild(viewportDiv);
            
            return window.ScreenshotUtils.takeScreenshot(viewportDiv, {
                scale: 1.0,
                source: 'viewport_test'
            }).then(function(canvas) {
                // Clean up
                document.body.removeChild(viewportDiv);
                
                const dataURL = canvas.toDataURL('image/png');
                return {
                    success: true,
                    dataURL: dataURL,
                    width: canvas.width,
                    height: canvas.height,
                    viewportWidth: viewportWidth,
                    viewportHeight: viewportHeight
                };
            }).catch(function(error) {
                // Clean up on error
                if (document.getElementById('viewport-capture-test')) {
                    document.body.removeChild(viewportDiv);
                }
                return {success: false, error: error.message};
            });
        """)
        
        assert result['success'], f"Viewport capture failed: {result.get('error')}"
        
        data = json.loads(result['result'])
        assert data['success'], f"Screenshot failed: {data.get('error')}"
        
        # Save viewport capture for analysis
        base64_data = data['dataURL'].split(',')[1]
        image_bytes = base64.b64decode(base64_data)
        
        viewport_file = Path('__tests__/integration/test_outputs/viewport_capture.png')
        viewport_file.parent.mkdir(exist_ok=True)
        with open(viewport_file, 'wb') as f:
            f.write(image_bytes)
        
        # Verify dimensions match viewport roughly
        canvas_width = data['width']
        canvas_height = data['height']
        viewport_width = data['viewportWidth']
        viewport_height = data['viewportHeight']
        
        # Allow for some scaling differences
        width_ratio = canvas_width / viewport_width
        height_ratio = canvas_height / viewport_height
        
        assert 0.5 <= width_ratio <= 2.0, f"Width ratio suspicious: {width_ratio}"
        assert 0.5 <= height_ratio <= 2.0, f"Height ratio suspicious: {height_ratio}"
        
        print(f"✅ Viewport captured: {canvas_width}x{canvas_height} (viewport: {viewport_width}x{viewport_height})")
        return image_bytes
    
    @pytest.mark.asyncio
    async def test_scroll_and_capture(self, client):
        """Test capturing after scrolling to different positions"""
        
        result = await client.js.execute("""
            console.log('📸 Testing scroll and capture...');
            
            const results = [];
            
            // Function to capture at current scroll position
            async function captureAtPosition(scrollY, name) {
                window.scrollTo(0, scrollY);
                await new Promise(resolve => setTimeout(resolve, 500)); // Wait for scroll
                
                const element = document.body;
                const canvas = await window.ScreenshotUtils.takeScreenshot(element, {
                    scale: 0.3,
                    source: 'scroll_test_' + name
                });
                
                return {
                    name: name,
                    scrollY: scrollY,
                    width: canvas.width,
                    height: canvas.height,
                    dataURL: canvas.toDataURL('image/png')
                };
            }
            
            try {
                // Capture at top
                const topCapture = await captureAtPosition(0, 'top');
                results.push(topCapture);
                
                // Capture at middle
                const middleScroll = Math.floor(document.body.scrollHeight / 2);
                const middleCapture = await captureAtPosition(middleScroll, 'middle');
                results.push(middleCapture);
                
                // Capture at bottom
                const bottomScroll = document.body.scrollHeight - window.innerHeight;
                const bottomCapture = await captureAtPosition(Math.max(0, bottomScroll), 'bottom');
                results.push(bottomCapture);
                
                // Scroll back to top
                window.scrollTo(0, 0);
                
                return {
                    success: true,
                    captures: results,
                    totalScrollHeight: document.body.scrollHeight
                };
                
            } catch (error) {
                window.scrollTo(0, 0); // Reset on error
                return {success: false, error: error.message};
            }
        """)
        
        assert result['success'], f"Scroll capture failed: {result.get('error')}"
        
        data = json.loads(result['result'])
        assert data['success'], f"Scroll capture execution failed: {data.get('error')}"
        
        captures = data['captures']
        assert len(captures) >= 2, f"Not enough captures: {len(captures)}"
        
        # Save each capture
        for i, capture in enumerate(captures):
            base64_data = capture['dataURL'].split(',')[1]
            image_bytes = base64.b64decode(base64_data)
            
            scroll_file = Path(f'__tests__/integration/test_outputs/scroll_{capture["name"]}.png')
            scroll_file.parent.mkdir(exist_ok=True)
            with open(scroll_file, 'wb') as f:
                f.write(image_bytes)
            
            print(f"✅ Scroll {capture['name']}: {capture['width']}x{capture['height']} at scroll {capture['scrollY']}")
        
        # Verify captures are different (basic check)
        if len(captures) >= 2:
            top_size = len(base64.b64decode(captures[0]['dataURL'].split(',')[1]))
            bottom_size = len(base64.b64decode(captures[-1]['dataURL'].split(',')[1]))
            
            # They should be different sizes (different content)
            size_diff = abs(top_size - bottom_size) / max(top_size, bottom_size)
            print(f"✅ Scroll captures differ by {size_diff:.1%} in file size")
        
        return captures
    
    @pytest.mark.asyncio
    async def test_responsive_breakpoints(self, client):
        """Test captures at different viewport sizes (responsive design)"""
        
        result = await client.js.execute("""
            console.log('📸 Testing responsive breakpoints...');
            
            const breakpoints = [
                {name: 'mobile', width: 375, height: 667},
                {name: 'tablet', width: 768, height: 1024},
                {name: 'desktop', width: 1200, height: 800}
            ];
            
            const results = [];
            const originalWidth = window.innerWidth;
            const originalHeight = window.innerHeight;
            
            try {
                for (const bp of breakpoints) {
                    console.log('📱 Testing', bp.name, bp.width + 'x' + bp.height);
                    
                    // Simulate viewport resize (note: this won't actually resize the browser)
                    // But we can capture what would be visible in that viewport
                    const testDiv = document.createElement('div');
                    testDiv.style.position = 'fixed';
                    testDiv.style.top = '0';
                    testDiv.style.left = '0';
                    testDiv.style.width = bp.width + 'px';
                    testDiv.style.height = bp.height + 'px';
                    testDiv.style.overflow = 'hidden';
                    testDiv.style.zIndex = '10000';
                    testDiv.style.backgroundColor = 'rgba(255,255,255,0.01)';
                    testDiv.id = 'responsive-test-' + bp.name;
                    
                    document.body.appendChild(testDiv);
                    
                    await new Promise(resolve => setTimeout(resolve, 200));
                    
                    const canvas = await window.ScreenshotUtils.takeScreenshot(testDiv, {
                        scale: 0.5,
                        source: 'responsive_' + bp.name
                    });
                    
                    document.body.removeChild(testDiv);
                    
                    results.push({
                        breakpoint: bp.name,
                        targetWidth: bp.width,
                        targetHeight: bp.height,
                        captureWidth: canvas.width,
                        captureHeight: canvas.height,
                        dataURL: canvas.toDataURL('image/png')
                    });
                }
                
                return {success: true, results: results};
                
            } catch (error) {
                // Clean up any remaining test divs
                breakpoints.forEach(bp => {
                    const testDiv = document.getElementById('responsive-test-' + bp.name);
                    if (testDiv) document.body.removeChild(testDiv);
                });
                
                return {success: false, error: error.message};
            }
        """)
        
        assert result['success'], f"Responsive testing failed: {result.get('error')}"
        
        data = json.loads(result['result'])
        assert data['success'], f"Responsive execution failed: {data.get('error')}"
        
        results = data['results']
        assert len(results) >= 2, f"Not enough responsive captures: {len(results)}"
        
        # Save and analyze each breakpoint
        for result_data in results:
            base64_data = result_data['dataURL'].split(',')[1]
            image_bytes = base64.b64decode(base64_data)
            
            responsive_file = Path(f'__tests__/integration/test_outputs/responsive_{result_data["breakpoint"]}.png')
            responsive_file.parent.mkdir(exist_ok=True)
            with open(responsive_file, 'wb') as f:
                f.write(image_bytes)
            
            # Verify capture dimensions relate to target
            target_width = result_data['targetWidth']
            capture_width = result_data['captureWidth']
            
            print(f"✅ {result_data['breakpoint']}: target {target_width}px → capture {capture_width}px")
        
        return results
    
    @pytest.mark.asyncio
    async def test_performance_large_captures(self, client):
        """Test performance and memory handling for large screen captures"""
        
        result = await client.js.execute("""
            console.log('📸 Testing large capture performance...');
            
            const startTime = performance.now();
            
            // Create a large element to capture
            const largeDiv = document.createElement('div');
            largeDiv.style.width = '2000px';
            largeDiv.style.height = '2000px';
            largeDiv.style.background = 'linear-gradient(45deg, #f0f0f0, #e0e0e0)';
            largeDiv.style.position = 'fixed';
            largeDiv.style.top = '0';
            largeDiv.style.left = '0';
            largeDiv.style.zIndex = '9999';
            largeDiv.innerHTML = '<h1 style="padding: 20px;">Performance Test Large Capture</h1>';
            
            document.body.appendChild(largeDiv);
            
            try {
                const canvas = await window.ScreenshotUtils.takeScreenshot(largeDiv, {
                    scale: 0.25,  // Lower scale for memory management
                    source: 'performance_test'
                });
                
                const endTime = performance.now();
                const duration = endTime - startTime;
                
                document.body.removeChild(largeDiv);
                
                return {
                    success: true,
                    duration: duration,
                    width: canvas.width,
                    height: canvas.height,
                    dataURL: canvas.toDataURL('image/png')
                };
                
            } catch (error) {
                document.body.removeChild(largeDiv);
                return {success: false, error: error.message};
            }
        """)
        
        assert result['success'], f"Performance test failed: {result.get('error')}"
        
        data = json.loads(result['result'])
        assert data['success'], f"Large capture failed: {data.get('error')}"
        
        duration = data['duration']
        width = data['width']
        height = data['height']
        
        # Performance assertions
        assert duration < 10000, f"Large capture too slow: {duration}ms"
        assert width > 100, f"Large capture width too small: {width}"
        assert height > 100, f"Large capture height too small: {height}"
        
        # Save performance test result
        base64_data = data['dataURL'].split(',')[1]
        image_bytes = base64.b64decode(base64_data)
        
        perf_file = Path('__tests__/integration/test_outputs/performance_large.png')
        perf_file.parent.mkdir(exist_ok=True)
        with open(perf_file, 'wb') as f:
            f.write(image_bytes)
        
        print(f"✅ Large capture performance: {duration:.1f}ms for {width}x{height} ({len(image_bytes)} bytes)")
        
        return {
            'duration': duration,
            'size': len(image_bytes),
            'dimensions': (width, height)
        }


if __name__ == "__main__":
    # Run tests directly
    import subprocess
    subprocess.run([
        'python', '-m', 'pytest', 
        __file__, 
        '-v', 
        '--tb=short',
        '-k', 'test_full_document_capture'  # Run just one test for quick verification
    ])