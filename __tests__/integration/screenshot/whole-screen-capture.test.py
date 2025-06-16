#!/usr/bin/env python3
"""
Whole Screen Capture Integration Test
Test capturing the entire browser window/screen
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


class TestWholeScreenCapture:
    """Test complete screen capture functionality"""
    
    @pytest.fixture
    async def client(self):
        """Set up continuum client for testing"""
        load_continuum_config()
        async with ContinuumClient() as client:
            await client.register_agent({
                'agentId': 'whole-screen-test',
                'agentName': 'Whole Screen Capture Test',
                'agentType': 'ai'
            })
            yield client
    
    @pytest.mark.asyncio
    async def test_whole_screen_capture_file_mode(self, client):
        """Test capturing the whole screen and saving to file"""
        
        print("📸 Testing whole screen capture (file mode)...")
        
        result = await client.js.execute("""
            console.log('📸 Whole screen capture test starting...');
            
            // Get screen/window dimensions
            const screenInfo = {
                windowWidth: window.innerWidth,
                windowHeight: window.innerHeight,
                screenWidth: window.screen ? window.screen.width : 'unknown',
                screenHeight: window.screen ? window.screen.height : 'unknown',
                documentWidth: Math.max(
                    document.documentElement.scrollWidth,
                    document.body.scrollWidth,
                    document.documentElement.offsetWidth,
                    document.body.offsetWidth
                ),
                documentHeight: Math.max(
                    document.documentElement.scrollHeight,
                    document.body.scrollHeight,
                    document.documentElement.offsetHeight,
                    document.body.offsetHeight
                )
            };
            
            console.log('📐 Screen info:', screenInfo);
            
            if (typeof window.continuum === 'undefined' || !window.continuum.command) {
                return {success: false, error: 'continuum.command not available'};
            }
            
            try {
                // Capture whole document/screen
                window.continuum.command.screenshot({
                    selector: 'body',  // This should capture the whole visible area
                    name_prefix: 'whole_screen_test',
                    scale: 0.5,  // Smaller scale for large captures
                    destination: 'file'
                });
                
                return {
                    success: true,
                    screenInfo: screenInfo,
                    message: 'Whole screen capture initiated'
                };
                
            } catch (error) {
                return {success: false, error: error.message};
            }
        """)
        
        assert result['success'], f"Whole screen capture failed: {result.get('error')}"
        
        data = json.loads(result['result'])
        assert data['success'], f"Screen capture command failed: {data.get('error')}"
        
        screen_info = data['screenInfo']
        print(f"📐 Window: {screen_info['windowWidth']}x{screen_info['windowHeight']}")
        print(f"📐 Document: {screen_info['documentWidth']}x{screen_info['documentHeight']}")
        
        # Wait for file creation
        print("⏳ Waiting for whole screen file creation...")
        await asyncio.sleep(5)  # Whole screen takes longer
        
        # Verify file was created
        screenshot_dir = Path('.continuum/screenshots')
        assert screenshot_dir.exists(), "Screenshots directory not found"
        
        files = list(screenshot_dir.glob('whole_screen_test_*.png'))
        assert len(files) > 0, "No whole screen file created"
        
        latest_file = max(files, key=lambda f: f.stat().st_mtime)
        
        # Verify it's a substantial capture
        with Image.open(latest_file) as img:
            width, height = img.size
            file_size = latest_file.stat().st_size
            
            print(f"✅ Whole screen file: {latest_file.name}")
            print(f"📐 Image dimensions: {width}x{height}")
            print(f"💾 File size: {file_size} bytes")
            
            # Should be substantial for whole screen
            assert width > 300, f"Whole screen too narrow: {width}px"
            assert height > 200, f"Whole screen too short: {height}px"
            assert file_size > 10000, f"Whole screen file too small: {file_size} bytes"
        
        return {
            'file': latest_file,
            'dimensions': (width, height),
            'file_size': file_size,
            'screen_info': screen_info
        }
    
    @pytest.mark.asyncio
    async def test_whole_screen_capture_bytes_mode(self, client):
        """Test capturing whole screen and returning bytes"""
        
        print("📸 Testing whole screen capture (bytes mode)...")
        
        result = await client.js.execute("""
            console.log('📸 Whole screen bytes capture starting...');
            
            if (typeof window.ScreenshotUtils === 'undefined') {
                return {success: false, error: 'ScreenshotUtils not available'};
            }
            
            // Capture the whole document body
            const targetElement = document.body;
            
            return window.ScreenshotUtils.takeScreenshot(targetElement, {
                scale: 0.3,  // Smaller scale for whole screen
                source: 'whole_screen_bytes_test'
            }).then(function(canvas) {
                const dataURL = canvas.toDataURL('image/png');
                
                // Get element info
                const bodyRect = targetElement.getBoundingClientRect();
                
                return {
                    success: true,
                    dataURL: dataURL,
                    canvasWidth: canvas.width,
                    canvasHeight: canvas.height,
                    elementInfo: {
                        offsetWidth: targetElement.offsetWidth,
                        offsetHeight: targetElement.offsetHeight,
                        scrollWidth: targetElement.scrollWidth,
                        scrollHeight: targetElement.scrollHeight,
                        rectWidth: bodyRect.width,
                        rectHeight: bodyRect.height
                    },
                    dataSize: dataURL.length
                };
            }).catch(function(error) {
                return {success: false, error: error.message};
            });
        """)
        
        assert result['success'], f"Bytes capture failed: {result.get('error')}"
        
        data = json.loads(result['result'])
        assert data['success'], f"Screenshot execution failed: {data.get('error')}"
        
        canvas_width = data['canvasWidth']
        canvas_height = data['canvasHeight']
        element_info = data['elementInfo']
        
        print(f"📐 Canvas: {canvas_width}x{canvas_height}")
        print(f"📐 Body element: {element_info['offsetWidth']}x{element_info['offsetHeight']}")
        print(f"📐 Body scroll: {element_info['scrollWidth']}x{element_info['scrollHeight']}")
        
        # Extract and save image
        base64_data = data['dataURL'].split(',')[1]
        image_bytes = base64.b64decode(base64_data)
        
        whole_screen_file = Path('__tests__/integration/test_outputs/whole_screen_bytes.png')
        whole_screen_file.parent.mkdir(exist_ok=True)
        with open(whole_screen_file, 'wb') as f:
            f.write(image_bytes)
        
        print(f"💾 Saved whole screen bytes: {len(image_bytes)} bytes")
        
        # Verify with PIL
        with Image.open(whole_screen_file) as img:
            verify_width, verify_height = img.size
            assert verify_width == canvas_width, "Width mismatch"
            assert verify_height == canvas_height, "Height mismatch"
        
        # Should be substantial
        assert canvas_width > 300, f"Canvas too narrow: {canvas_width}px"
        assert canvas_height > 200, f"Canvas too short: {canvas_height}px"
        assert len(image_bytes) > 5000, f"Image data too small: {len(image_bytes)} bytes"
        
        print("✅ Whole screen bytes capture successful")
        
        return {
            'image_bytes': image_bytes,
            'dimensions': (canvas_width, canvas_height),
            'element_info': element_info
        }
    
    @pytest.mark.asyncio
    async def test_screen_content_analysis(self, client):
        """Test analyzing the content of the whole screen capture"""
        
        print("🔍 Testing whole screen content analysis...")
        
        # First capture the screen
        bytes_result = await self.test_whole_screen_capture_bytes_mode(client)
        image_bytes = bytes_result['image_bytes']
        
        # Save for analysis
        analysis_file = Path('__tests__/integration/test_outputs/screen_content_analysis.png')
        analysis_file.parent.mkdir(exist_ok=True)
        with open(analysis_file, 'wb') as f:
            f.write(image_bytes)
        
        # Use OpenCV for content analysis
        img = cv2.imread(str(analysis_file))
        if img is None:
            pytest.fail("Could not load captured image for analysis")
        
        height, width, channels = img.shape
        print(f"📐 Analysis image: {width}x{height}, {channels} channels")
        
        # Convert to grayscale for analysis
        gray = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
        
        # Basic content analysis
        analysis_results = {
            'dimensions': (width, height),
            'file_size': len(image_bytes),
            'has_content': True,  # If we got here, we have an image
            'brightness': {
                'mean': float(np.mean(gray)),
                'std': float(np.std(gray)),
                'min': int(np.min(gray)),
                'max': int(np.max(gray))
            }
        }
        
        # Check if image is mostly blank (indicating capture problems)
        if analysis_results['brightness']['std'] < 10:
            print("⚠️ Warning: Image appears mostly uniform (possible capture issue)")
            analysis_results['likely_blank'] = True
        else:
            print("✅ Image has good content variation")
            analysis_results['likely_blank'] = False
        
        # Edge detection to find UI elements
        edges = cv2.Canny(gray, 50, 150)
        edge_count = np.sum(edges > 0)
        edge_percentage = edge_count / (width * height) * 100
        
        analysis_results['edge_analysis'] = {
            'edge_pixels': int(edge_count),
            'edge_percentage': round(edge_percentage, 2)
        }
        
        print(f"🔍 Content analysis:")
        print(f"  📊 Brightness: mean={analysis_results['brightness']['mean']:.1f}, std={analysis_results['brightness']['std']:.1f}")
        print(f"  🔲 Edges: {edge_percentage:.1f}% of pixels")
        
        # Save edge detection result
        edge_file = Path('__tests__/integration/test_outputs/screen_edges.png')
        cv2.imwrite(str(edge_file), edges)
        print(f"💾 Edge analysis saved: {edge_file}")
        
        # Verify we captured actual content
        assert not analysis_results['likely_blank'], "Screen capture appears blank"
        assert edge_percentage > 0.5, f"Too few edges detected: {edge_percentage:.1f}%"
        
        print("✅ Screen content analysis completed")
        return analysis_results
    
    @pytest.mark.asyncio
    async def test_screen_size_consistency(self, client):
        """Test that file mode and bytes mode produce consistent screen captures"""
        
        print("🔄 Testing screen capture consistency between modes...")
        
        # Get both modes
        file_result = await self.test_whole_screen_capture_file_mode(client)
        bytes_result = await self.test_whole_screen_capture_bytes_mode(client)
        
        file_dims = file_result['dimensions']
        bytes_dims = bytes_result['dimensions']
        file_size = file_result['file_size']
        bytes_size = len(bytes_result['image_bytes'])
        
        print(f"📊 Consistency check:")
        print(f"  📁 File mode: {file_dims[0]}x{file_dims[1]} ({file_size} bytes)")
        print(f"  💾 Bytes mode: {bytes_dims[0]}x{bytes_dims[1]} ({bytes_size} bytes)")
        
        # Check dimension consistency (allowing for scale differences)
        width_ratio = file_dims[0] / bytes_dims[0] if bytes_dims[0] > 0 else 0
        height_ratio = file_dims[1] / bytes_dims[1] if bytes_dims[1] > 0 else 0
        
        print(f"  📐 Dimension ratios: width={width_ratio:.2f}, height={height_ratio:.2f}")
        
        # Ratios should be similar (same scale factor applied)
        ratio_diff = abs(width_ratio - height_ratio)
        assert ratio_diff < 0.2, f"Inconsistent scaling between modes: {ratio_diff}"
        
        # File sizes should be in reasonable range
        size_ratio = file_size / bytes_size if bytes_size > 0 else 0
        print(f"  💾 Size ratio: {size_ratio:.2f}")
        
        # Sizes should be reasonably close (within 50%)
        assert 0.5 <= size_ratio <= 2.0, f"File sizes too different: {size_ratio}"
        
        print("✅ Screen capture consistency verified")
        
        return {
            'file_mode': file_result,
            'bytes_mode': bytes_result,
            'consistency': {
                'width_ratio': width_ratio,
                'height_ratio': height_ratio,
                'size_ratio': size_ratio,
                'ratio_diff': ratio_diff
            }
        }


if __name__ == "__main__":
    # Run tests directly
    import subprocess
    subprocess.run([
        'python', '-m', 'pytest', 
        __file__, 
        '-v', 
        '--tb=short',
        '-k', 'test_whole_screen_capture_file_mode'  # Run one test for quick check
    ])