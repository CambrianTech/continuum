"""
Unit tests for elegant screenshot utilities
"""

import pytest
import asyncio
import sys
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

# Add continuum_client to path
project_root = Path(__file__).parent.parent.parent.parent.parent
sys.path.insert(0, str(project_root / 'python-client'))

from continuum_client.utils.screenshot import ScreenshotCapture, capture_version_badge, capture_users_agents


class TestScreenshotCapture:
    """Test cases for ScreenshotCapture class"""
    
    def test_init_default_directory(self):
        """Test initialization with default screenshot directory"""
        with patch('pathlib.Path.cwd') as mock_cwd:
            mock_cwd.return_value = MagicMock()
            mock_cwd.return_value.parent = MagicMock()
            
            capture = ScreenshotCapture()
            assert capture.screenshots_dir is not None
    
    def test_selector_to_name_common_selectors(self):
        """Test conversion of common CSS selectors to filesystem names"""
        capture = ScreenshotCapture()
        
        # Test common selector mappings
        assert capture._selector_to_name('.version-badge') == 'version'
        assert capture._selector_to_name('.users-agents') == 'users_agents'
        assert capture._selector_to_name('body') == 'fullpage'
        assert capture._selector_to_name('#sidebar') == 'sidebar'
    
    def test_selector_to_name_generic_cleanup(self):
        """Test generic cleanup for arbitrary selectors"""
        capture = ScreenshotCapture()
        
        # Test generic selector cleanup
        assert capture._selector_to_name('.some-class') == 'some_class'
        assert capture._selector_to_name('#some-id') == 'some_id'
        assert capture._selector_to_name('[data-test="value"]') == 'data_test_value'
        assert capture._selector_to_name('') == 'element'
    
    @pytest.mark.asyncio
    async def test_capture_by_selector_success(self):
        """Test successful screenshot capture by selector"""
        # Mock client
        mock_client = MagicMock()
        mock_client.js.execute = AsyncMock()
        
        # Mock successful JavaScript execution
        mock_client.js.execute.return_value = {
            'success': True,
            'result': '{"success": true, "dataURL": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==", "width": 100, "height": 100, "filename": "test.png"}'
        }
        
        # Create capture instance with temporary directory
        with patch('pathlib.Path') as mock_path:
            mock_screenshots_dir = MagicMock()
            mock_screenshots_dir.mkdir = MagicMock()
            mock_path.return_value = mock_screenshots_dir
            
            capture = ScreenshotCapture()
            
            # Mock file writing
            with patch('builtins.open', create=True) as mock_open:
                mock_file = MagicMock()
                mock_open.return_value.__enter__.return_value = mock_file
                
                result = await capture.capture_by_selector(mock_client, '.test-selector')
        
        # Verify result
        assert result['success'] is True
        assert 'filename' in result
        assert 'path' in result
        assert result['selector'] == '.test-selector'
        assert result['width'] == 100
        assert result['height'] == 100
    
    @pytest.mark.asyncio
    async def test_capture_by_selector_js_failure(self):
        """Test screenshot capture when JavaScript execution fails"""
        # Mock client with failed JavaScript execution
        mock_client = MagicMock()
        mock_client.js.execute = AsyncMock()
        mock_client.js.execute.return_value = {
            'success': False,
            'error': 'JavaScript execution failed'
        }
        
        capture = ScreenshotCapture()
        result = await capture.capture_by_selector(mock_client, '.test-selector')
        
        # Verify failure is handled properly
        assert result['success'] is False
        assert 'error' in result
    
    @pytest.mark.asyncio
    async def test_capture_by_selector_screenshot_failure(self):
        """Test screenshot capture when html2canvas fails"""
        # Mock client
        mock_client = MagicMock()
        mock_client.js.execute = AsyncMock()
        
        # Mock JavaScript execution with screenshot failure
        mock_client.js.execute.return_value = {
            'success': True,
            'result': '{"success": false, "error": "Element not found"}'
        }
        
        capture = ScreenshotCapture()
        result = await capture.capture_by_selector(mock_client, '.nonexistent-selector')
        
        # Verify failure is handled properly
        assert result['success'] is False
        assert 'error' in result


class TestConvenienceFunctions:
    """Test cases for convenience functions"""
    
    @pytest.mark.asyncio
    async def test_capture_version_badge(self):
        """Test version badge capture convenience function"""
        mock_client = MagicMock()
        
        with patch('continuum_client.utils.screenshot.ScreenshotCapture') as mock_capture_class:
            mock_capture = MagicMock()
            mock_capture_class.return_value = mock_capture
            
            # Mock successful capture
            mock_capture.capture_by_selector = AsyncMock()
            mock_capture.capture_by_selector.return_value = {
                'success': True,
                'filename': 'version_badge_20231215_120000.png'
            }
            
            result = await capture_version_badge(mock_client)
            
            # Verify correct parameters were passed
            mock_capture.capture_by_selector.assert_called_once_with(
                mock_client,
                selector='.version-badge',
                name_prefix='version',
                scale=2.0
            )
            
            assert result['success'] is True
    
    @pytest.mark.asyncio
    async def test_capture_users_agents_fallback(self):
        """Test users & agents capture with fallback logic"""
        mock_client = MagicMock()
        
        with patch('continuum_client.utils.screenshot.ScreenshotCapture') as mock_capture_class:
            mock_capture = MagicMock()
            mock_capture_class.return_value = mock_capture
            
            # Mock first few selectors failing, last one succeeding
            mock_capture.capture_by_selector = AsyncMock()
            mock_capture.capture_by_selector.side_effect = [
                {'success': False, 'error': 'Selector not found'},  # First selector fails
                {'success': False, 'error': 'Selector not found'},  # Second selector fails  
                {'success': False, 'error': 'Selector not found'},  # Third selector fails
                {'success': True, 'filename': 'users_agents_sidebar_20231215_120000.png'}  # Fourth succeeds
            ]
            
            result = await capture_users_agents(mock_client)
            
            # Verify fallback logic worked
            assert mock_capture.capture_by_selector.call_count == 4
            assert result['success'] is True


if __name__ == '__main__':
    # Run tests
    pytest.main([__file__, '-v'])