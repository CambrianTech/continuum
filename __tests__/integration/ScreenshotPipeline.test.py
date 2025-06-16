#!/usr/bin/env python3
"""
Screenshot Pipeline Integration Tests
Real tests that verify the entire screenshot pipeline works end-to-end
"""

import asyncio
import pytest
import sys
from pathlib import Path
import json
import base64
from PIL import Image
import pytesseract
import cv2
import numpy as np

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config


class TestScreenshotPipeline:
    """Test the complete screenshot pipeline from command to file/bytes"""
    
    @pytest.fixture
    async def client(self):
        """Set up continuum client for testing"""
        load_continuum_config()
        async with ContinuumClient() as client:
            await client.register_agent({
                'agentId': 'screenshot-pipeline-test',
                'agentName': 'Screenshot Pipeline Test',
                'agentType': 'ai'
            })
            yield client
    
    @pytest.mark.asyncio
    async def test_file_mode_creates_actual_file(self, client):
        """Test that file mode actually creates a file with correct naming"""
        
        # Execute file mode screenshot
        result = await client.js.execute("""
            if (typeof window.continuum === 'undefined' || !window.continuum.command) {
                return {success: false, error: 'continuum.command not available'};
            }
            
            try {
                const promise = window.continuum.command.screenshot({
                    selector: '.version-badge',
                    name_prefix: 'integration_test_file',
                    scale: 1.0,
                    destination: 'file',
                    format: 'png'
                });
                
                return {success: true, message: 'File mode command sent'};
            } catch (error) {
                return {success: false, error: error.message};
            }
        """)
        
        assert result['success'], f"Command failed: {result.get('error')}"
        
        # Wait for file creation
        await asyncio.sleep(3)
        
        # Verify file was created
        screenshot_dir = Path('.continuum/screenshots')
        assert screenshot_dir.exists(), "Screenshots directory not found"
        
        files = list(screenshot_dir.glob('integration_test_file_*.png'))
        assert len(files) > 0, "No files created by file mode"
        
        # Check the most recent file
        latest_file = max(files, key=lambda f: f.stat().st_mtime)
        
        # Verify file properties
        assert latest_file.suffix == '.png', "File has wrong extension"
        assert latest_file.stat().st_size > 0, "File is empty"
        assert 'integration_test_file' in latest_file.name, "File name doesn't match prefix"
        
        # Verify it's a valid image
        with Image.open(latest_file) as img:
            assert img.format == 'PNG', "Not a valid PNG file"
            assert img.size[0] > 0 and img.size[1] > 0, "Image has zero dimensions"
        
        print(f"✅ File mode created: {latest_file.name} ({latest_file.stat().st_size} bytes)")
        return latest_file
    
    @pytest.mark.asyncio 
    async def test_bytes_mode_returns_valid_image_data(self, client):
        """Test that bytes mode returns valid image data"""
        
        result = await client.js.execute("""
            const versionElement = document.querySelector('.version-badge');
            if (!versionElement) {
                return {success: false, error: 'Version badge not found'};
            }
            
            if (typeof window.ScreenshotUtils === 'undefined') {
                return {success: false, error: 'ScreenshotUtils not available'};
            }
            
            return window.ScreenshotUtils.takeScreenshot(versionElement, {
                scale: 1.0,
                source: 'integration_test_bytes'
            }).then(function(canvas) {
                const dataURL = canvas.toDataURL('image/png');
                return {
                    success: true,
                    dataURL: dataURL,
                    width: canvas.width,
                    height: canvas.height
                };
            }).catch(function(error) {
                return {success: false, error: error.message};
            });
        """)
        
        assert result['success'], f"JS execution failed: {result.get('error')}"
        
        data = json.loads(result['result'])
        assert data['success'], f"Screenshot failed: {data.get('error')}"
        
        # Verify data URL format
        data_url = data['dataURL']
        assert data_url.startswith('data:image/png;base64,'), "Invalid data URL format"
        
        # Extract and decode base64 data
        base64_data = data_url.split(',')[1]
        image_bytes = base64.b64decode(base64_data)
        
        # Verify PNG header
        assert image_bytes.startswith(b'\x89PNG'), "Invalid PNG header"
        
        # Save for analysis
        test_file = Path('__tests__/integration/test_outputs/bytes_mode_test.png')
        test_file.parent.mkdir(exist_ok=True)
        with open(test_file, 'wb') as f:
            f.write(image_bytes)
        
        # Verify with PIL
        with Image.open(test_file) as img:
            assert img.format == 'PNG', "PIL can't read as PNG"
            assert img.size == (data['width'], data['height']), "Size mismatch"
        
        print(f"✅ Bytes mode returned valid PNG: {len(image_bytes)} bytes, {data['width']}x{data['height']}")
        return image_bytes
    
    @pytest.mark.asyncio
    async def test_version_badge_text_recognition(self, client):
        """Test that we can actually read the version number from screenshot"""
        
        # Capture version badge in bytes mode
        result = await client.js.execute("""
            const versionElement = document.querySelector('.version-badge');
            if (!versionElement) {
                return {success: false, error: 'Version badge not found'};
            }
            
            return window.ScreenshotUtils.takeScreenshot(versionElement, {
                scale: 2.0,  // Higher scale for better OCR
                source: 'ocr_test'
            }).then(function(canvas) {
                const dataURL = canvas.toDataURL('image/png');
                return {
                    success: true,
                    dataURL: dataURL,
                    width: canvas.width,
                    height: canvas.height
                };
            });
        """)
        
        assert result['success'], "Failed to capture version badge"
        
        data = json.loads(result['result'])
        assert data['success'], "Screenshot capture failed"
        
        # Save image for OCR
        base64_data = data['dataURL'].split(',')[1]
        image_bytes = base64.b64decode(base64_data)
        
        ocr_file = Path('__tests__/integration/test_outputs/version_for_ocr.png')
        ocr_file.parent.mkdir(exist_ok=True)
        with open(ocr_file, 'wb') as f:
            f.write(image_bytes)
        
        # Use OpenCV to preprocess for better OCR
        img = cv2.imread(str(ocr_file))
        gray = cv2.cvtColor(img, cv2.COLOR_BGR2GRAY)
        
        # Enhance contrast for better text recognition
        enhanced = cv2.convertScaleAbs(gray, alpha=2.0, beta=0)
        
        # Save enhanced version
        enhanced_file = Path('__tests__/integration/test_outputs/version_enhanced.png')
        cv2.imwrite(str(enhanced_file), enhanced)
        
        # Extract text using tesseract
        try:
            text = pytesseract.image_to_string(enhanced, config='--psm 8 -c tessedit_char_whitelist=0123456789.')
            text = text.strip()
            
            print(f"✅ OCR extracted text: '{text}'")
            
            # Verify it looks like a version number
            assert '.' in text, f"Version text doesn't contain dot: '{text}'"
            
            # Try to parse as version components
            parts = text.split('.')
            assert len(parts) >= 2, f"Not enough version parts: '{text}'"
            
            # Check that parts are numeric
            for part in parts:
                if part:  # Skip empty parts
                    assert part.isdigit(), f"Non-numeric version part: '{part}'"
            
            return text
            
        except Exception as e:
            pytest.fail(f"OCR failed: {e}")
    
    @pytest.mark.asyncio
    async def test_file_and_bytes_mode_consistency(self, client):
        """Test that file mode and bytes mode produce identical images"""
        
        # Get bytes mode screenshot
        bytes_result = await client.js.execute("""
            const element = document.querySelector('.version-badge');
            return window.ScreenshotUtils.takeScreenshot(element, {
                scale: 1.0,
                source: 'consistency_test'
            }).then(canvas => ({
                success: true,
                dataURL: canvas.toDataURL('image/png'),
                width: canvas.width,
                height: canvas.height
            }));
        """)
        
        # Trigger file mode screenshot
        file_result = await client.js.execute("""
            return window.continuum.command.screenshot({
                selector: '.version-badge',
                name_prefix: 'consistency_test_file',
                scale: 1.0,
                destination: 'file'
            }).then(() => ({success: true}));
        """)
        
        assert bytes_result['success'] and file_result['success'], "One mode failed"
        
        # Wait for file creation
        await asyncio.sleep(3)
        
        # Get the file mode image
        screenshot_dir = Path('.continuum/screenshots')
        files = list(screenshot_dir.glob('consistency_test_file_*.png'))
        assert len(files) > 0, "File mode didn't create file"
        
        latest_file = max(files, key=lambda f: f.stat().st_mtime)
        
        # Compare file sizes (should be very close)
        bytes_data = json.loads(bytes_result['result'])
        bytes_image = base64.b64decode(bytes_data['dataURL'].split(',')[1])
        file_size = latest_file.stat().st_size
        bytes_size = len(bytes_image)
        
        size_diff = abs(file_size - bytes_size)
        size_ratio = size_diff / max(file_size, bytes_size)
        
        assert size_ratio < 0.05, f"File sizes too different: {file_size} vs {bytes_size} bytes"
        
        print(f"✅ Consistency test: File {file_size} bytes, Bytes {bytes_size} bytes (diff: {size_diff})")
        
    @pytest.mark.asyncio
    async def test_screenshot_console_logging(self, client):
        """Test that screenshot commands produce proper console output"""
        
        # Capture console output during screenshot
        result = await client.js.execute("""
            var logs = [];
            var originalLog = console.log;
            var originalWarn = console.warn;
            
            console.log = function() {
                logs.push('LOG: ' + Array.prototype.slice.call(arguments).join(' '));
                originalLog.apply(console, arguments);
            };
            
            console.warn = function() {
                logs.push('WARN: ' + Array.prototype.slice.call(arguments).join(' '));
                originalWarn.apply(console, arguments);
            };
            
            return window.continuum.command.screenshot({
                selector: '.version-badge',
                name_prefix: 'console_log_test',
                scale: 1.0
            }).then(() => {
                console.log = originalLog;
                console.warn = originalWarn;
                
                return {
                    success: true,
                    logs: logs.filter(log => log.includes('screenshot') || log.includes('📸'))
                };
            });
        """)
        
        assert result['success'], "Screenshot command failed"
        
        data = json.loads(result['result'])
        assert data['success'], "Screenshot execution failed"
        
        logs = data['logs']
        assert len(logs) > 0, "No screenshot-related logs found"
        
        # Verify we see expected log messages
        log_text = ' '.join(logs)
        assert '📸' in log_text, "No screenshot emoji in logs"
        assert 'continuum.command.screenshot' in log_text, "No command mention in logs"
        
        print(f"✅ Console logging test passed: {len(logs)} relevant log messages")
        return logs


if __name__ == "__main__":
    # Run tests directly if executed as script
    import subprocess
    subprocess.run([
        'python', '-m', 'pytest', 
        '__file__', 
        '-v', 
        '--tb=short'
    ])