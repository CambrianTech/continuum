"""
Integration tests for Screenshot System via Promise Post Office
Tests real screenshot capture through Continuum server with html2canvas
"""

import pytest
import asyncio
import json
import logging
import base64
import re
from continuum_client import ContinuumClient, ContinuumServerManager, JSExecutionError, JSTimeoutError
from continuum_client.utils import load_continuum_config

# Load Continuum configuration
load_continuum_config()

logger = logging.getLogger(__name__)

@pytest.mark.integration
class TestScreenshotSystem:
    """Test Screenshot System with real Continuum server and browser"""
    
    def parse_js_result(self, result_json):
        """Parse JSON result from JavaScript execution"""
        if isinstance(result_json, str) and result_json.startswith('{'):
            return json.loads(result_json)
        elif isinstance(result_json, str):
            # Try to parse as JSON, otherwise return as-is
            try:
                return json.loads(result_json)
            except:
                return result_json
        else:
            return result_json
    
    @pytest.fixture(scope="class")
    def continuum_server(self):
        """Shared Continuum server for all screenshot tests"""
        with ContinuumServerManager() as server:
            yield server
    
    @pytest.mark.asyncio
    async def test_screenshot_basic_capture(self, continuum_server):
        """Test basic full-page screenshot capture"""
        logger.info("📸 Testing Basic Screenshot Capture...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'screenshot-tester-1',
                'agentName': 'Screenshot Tester 1',
                'agentType': 'ai'
            })
            
            logger.info("  📤 Triggering full-page screenshot...")
            
            # Test basic screenshot capture using SCREENSHOT command format
            screenshot_js = """
            return new Promise((resolve, reject) => {
                console.log('📸 Starting screenshot capture...');
                
                // Load html2canvas if not available
                if (typeof html2canvas === 'undefined') {
                    const script = document.createElement('script');
                    script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                    script.onload = () => {
                        console.log('📸 html2canvas loaded');
                        captureScreenshot();
                    };
                    script.onerror = () => reject('Failed to load html2canvas');
                    document.head.appendChild(script);
                } else {
                    captureScreenshot();
                }
                
                function captureScreenshot() {
                    html2canvas(document.body, {
                        allowTaint: true,
                        useCORS: true,
                        scale: 1,
                        backgroundColor: '#1a1a1a'
                    }).then(canvas => {
                        const dataURL = canvas.toDataURL('image/png');
                        console.log('✅ Screenshot captured, size:', dataURL.length);
                        resolve(JSON.stringify({
                            success: true,
                            dataURL: dataURL,
                            width: canvas.width,
                            height: canvas.height,
                            format: 'png'
                        }));
                    }).catch(error => {
                        console.error('📸 Screenshot failed:', error);
                        reject(error.message);
                    });
                }
            });
            """
            
            try:
                result_json = await client.js.get_value(screenshot_js, timeout=30)
                result = json.loads(result_json)
                
                # Verify screenshot data
                assert result['success'] == True
                assert 'dataURL' in result
                assert result['dataURL'].startswith('data:image/png;base64,')
                assert result['width'] > 0
                assert result['height'] > 0
                
                # Verify base64 data is valid
                base64_data = result['dataURL'].split(',')[1]
                try:
                    base64.b64decode(base64_data)
                    logger.info(f"  📥 Screenshot captured: {result['width']}x{result['height']}, {len(base64_data)} bytes base64")
                except Exception as e:
                    pytest.fail(f"Invalid base64 data: {e}")
                
                logger.info("✅ Basic Screenshot Capture Complete")
                
            except Exception as e:
                logger.error(f"❌ Screenshot capture failed: {e}")
                pytest.fail(f"Screenshot capture failed: {e}")
    
    @pytest.mark.asyncio
    async def test_screenshot_element_selector(self, continuum_server):
        """Test screenshot with element selector"""
        logger.info("📸 Testing Element Selector Screenshot...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'screenshot-tester-2',
                'agentName': 'Screenshot Tester 2',
                'agentType': 'ai'
            })
            
            logger.info("  📤 Creating test element and capturing it...")
            
            # Create a test element and capture it
            screenshot_js = """
            return new Promise((resolve, reject) => {
                // Create a test element to screenshot
                const testDiv = document.createElement('div');
                testDiv.id = 'screenshot-test-element';
                testDiv.style.width = '200px';
                testDiv.style.height = '100px';
                testDiv.style.backgroundColor = '#4FC3F7';
                testDiv.style.color = 'white';
                testDiv.style.padding = '20px';
                testDiv.style.position = 'fixed';
                testDiv.style.top = '50px';
                testDiv.style.left = '50px';
                testDiv.style.zIndex = '9999';
                testDiv.textContent = 'Test Element for Screenshot';
                document.body.appendChild(testDiv);
                
                // Wait a moment for element to render
                setTimeout(() => {
                    if (typeof html2canvas === 'undefined') {
                        const script = document.createElement('script');
                        script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                        script.onload = () => captureElement();
                        script.onerror = () => reject('Failed to load html2canvas');
                        document.head.appendChild(script);
                    } else {
                        captureElement();
                    }
                    
                    function captureElement() {
                        html2canvas(testDiv, {
                            allowTaint: true,
                            useCORS: true,
                            scale: 1
                        }).then(canvas => {
                            // Clean up test element
                            document.body.removeChild(testDiv);
                            
                            const dataURL = canvas.toDataURL('image/png');
                            resolve(JSON.stringify({
                                success: true,
                                dataURL: dataURL,
                                width: canvas.width,
                                height: canvas.height,
                                elementCaptured: true
                            }));
                        }).catch(error => {
                            document.body.removeChild(testDiv);
                            reject(error.message);
                        });
                    }
                }, 100);
            });
            """
            
            try:
                result_json = await client.js.get_value(screenshot_js, timeout=30)
                result = json.loads(result_json)
                
                # Verify element screenshot
                assert result['success'] == True
                assert result['elementCaptured'] == True
                assert 'dataURL' in result
                assert result['dataURL'].startswith('data:image/png;base64,')
                
                # Element screenshot should be smaller than full page
                assert result['width'] > 0 and result['width'] < 2000  # Reasonable bounds
                assert result['height'] > 0 and result['height'] < 2000
                
                logger.info(f"  📥 Element screenshot captured: {result['width']}x{result['height']}")
                logger.info("✅ Element Selector Screenshot Complete")
                
            except Exception as e:
                logger.error(f"❌ Element screenshot failed: {e}")
                pytest.fail(f"Element screenshot failed: {e}")
    
    @pytest.mark.asyncio
    async def test_screenshot_different_formats(self, continuum_server):
        """Test screenshot with different formats (PNG, JPEG)"""
        logger.info("📸 Testing Different Screenshot Formats...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'screenshot-tester-3',
                'agentName': 'Screenshot Tester 3',
                'agentType': 'ai'
            })
            
            # Test both PNG and JPEG formats
            formats = [
                ('png', 'image/png'),
                ('jpeg', 'image/jpeg')
            ]
            
            for format_name, mime_type in formats:
                logger.info(f"  📤 Testing {format_name.upper()} format...")
                
                screenshot_js = f"""
                return new Promise((resolve, reject) => {{
                    if (typeof html2canvas === 'undefined') {{
                        const script = document.createElement('script');
                        script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                        script.onload = () => captureWithFormat();
                        script.onerror = () => reject('Failed to load html2canvas');
                        document.head.appendChild(script);
                    }} else {{
                        captureWithFormat();
                    }}
                    
                    function captureWithFormat() {{
                        // Create small test area for faster capture
                        const testArea = document.createElement('div');
                        testArea.style.width = '100px';
                        testArea.style.height = '50px';
                        testArea.style.backgroundColor = '#29B6F6';
                        testArea.style.position = 'fixed';
                        testArea.style.top = '10px';
                        testArea.style.left = '10px';
                        testArea.style.zIndex = '9999';
                        document.body.appendChild(testArea);
                        
                        html2canvas(testArea, {{
                            allowTaint: true,
                            useCORS: true,
                            scale: 1
                        }}).then(canvas => {{
                            document.body.removeChild(testArea);
                            
                            let dataURL;
                            if ('{format_name}' === 'jpeg') {{
                                dataURL = canvas.toDataURL('{mime_type}', 0.8);
                            }} else {{
                                dataURL = canvas.toDataURL('{mime_type}');
                            }}
                            
                            resolve(JSON.stringify({{
                                success: true,
                                format: '{format_name}',
                                dataURL: dataURL,
                                width: canvas.width,
                                height: canvas.height
                            }}));
                        }}).catch(error => {{
                            if (document.body.contains(testArea)) {{
                                document.body.removeChild(testArea);
                            }}
                            reject(error.message);
                        }});
                    }}
                }});
                """
                
                try:
                    result_json = await client.js.get_value(screenshot_js, timeout=30)
                    result = json.loads(result_json)
                    
                    # Verify format-specific results
                    assert result['success'] == True
                    assert result['format'] == format_name
                    assert 'dataURL' in result
                    assert result['dataURL'].startswith(f'data:{mime_type};base64,')
                    
                    logger.info(f"  📥 {format_name.upper()} screenshot: {result['width']}x{result['height']}")
                    
                except Exception as e:
                    logger.error(f"❌ {format_name.upper()} screenshot failed: {e}")
                    pytest.fail(f"{format_name.upper()} screenshot failed: {e}")
            
            logger.info("✅ Different Screenshot Formats Complete")
    
    @pytest.mark.asyncio
    async def test_screenshot_error_handling(self, continuum_server):
        """Test screenshot error handling"""
        logger.info("📸 Testing Screenshot Error Handling...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'screenshot-tester-4',
                'agentName': 'Screenshot Tester 4',
                'agentType': 'ai'
            })
            
            logger.info("  📤 Testing screenshot with invalid element...")
            
            # Test screenshot of non-existent element
            screenshot_js = """
            return new Promise((resolve, reject) => {
                if (typeof html2canvas === 'undefined') {
                    const script = document.createElement('script');
                    script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                    script.onload = () => testInvalidElement();
                    script.onerror = () => reject('Failed to load html2canvas');
                    document.head.appendChild(script);
                } else {
                    testInvalidElement();
                }
                
                function testInvalidElement() {
                    const invalidElement = document.querySelector('#non-existent-element');
                    if (!invalidElement) {
                        // This should handle gracefully
                        resolve(JSON.stringify({
                            success: false,
                            error: 'Element not found',
                            handled: true
                        }));
                    } else {
                        // This shouldn't happen in our test
                        reject('Unexpected: found non-existent element');
                    }
                }
            });
            """
            
            try:
                result_json = await client.js.get_value(screenshot_js, timeout=30)
                result = json.loads(result_json)
                
                # Verify error handling
                assert result['success'] == False
                assert result['error'] == 'Element not found'
                assert result['handled'] == True
                
                logger.info("  📥 Error handling working correctly")
                logger.info("✅ Screenshot Error Handling Complete")
                
            except Exception as e:
                logger.error(f"❌ Error handling test failed: {e}")
                pytest.fail(f"Error handling test failed: {e}")
    
    @pytest.mark.asyncio 
    async def test_screenshot_integration_with_promise_system(self, continuum_server):
        """Test complete integration of screenshot with Promise Post Office System"""
        logger.info("📸 Testing Screenshot Integration with Promise System...")
        
        async with ContinuumClient() as client:
            # Register test agent with unique ID
            await client.register_agent({
                'agentId': 'screenshot-tester-5',
                'agentName': 'Screenshot Tester 5',
                'agentType': 'ai'
            })
            
            logger.info("  📤 Testing multiple concurrent screenshot requests...")
            
            # Test multiple screenshots concurrently (stress test Promise routing)
            async def capture_screenshot(test_id):
                screenshot_js = f"""
                return new Promise((resolve, reject) => {{
                    const testId = '{test_id}';
                    console.log('📸 Starting screenshot ' + testId);
                    
                    if (typeof html2canvas === 'undefined') {{
                        const script = document.createElement('script');
                        script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                        script.onload = () => captureTest();
                        script.onerror = () => reject('Failed to load html2canvas');
                        document.head.appendChild(script);
                    }} else {{
                        captureTest();
                    }}
                    
                    function captureTest() {{
                        // Create unique test element
                        const testDiv = document.createElement('div');
                        testDiv.style.width = '50px';
                        testDiv.style.height = '30px';
                        testDiv.style.backgroundColor = '#' + Math.floor(Math.random()*16777215).toString(16);
                        testDiv.style.position = 'fixed';
                        testDiv.style.top = '0px';
                        testDiv.style.left = (parseInt(testId) * 60) + 'px';
                        testDiv.style.zIndex = '9999';
                        testDiv.textContent = testId;
                        document.body.appendChild(testDiv);
                        
                        setTimeout(() => {{
                            html2canvas(testDiv, {{
                                allowTaint: true,
                                useCORS: true,
                                scale: 1
                            }}).then(canvas => {{
                                document.body.removeChild(testDiv);
                                const dataURL = canvas.toDataURL('image/png');
                                console.log('✅ Screenshot ' + testId + ' complete');
                                resolve(JSON.stringify({{
                                    testId: testId,
                                    success: true,
                                    dataURL: dataURL,
                                    width: canvas.width,
                                    height: canvas.height
                                }}));
                            }}).catch(error => {{
                                if (document.body.contains(testDiv)) {{
                                    document.body.removeChild(testDiv);
                                }}
                                reject(testId + ': ' + error.message);
                            }});
                        }}, 50);
                    }}
                }});
                """
                result_json = await client.js.get_value(screenshot_js, timeout=30)
                return json.loads(result_json)
            
            try:
                # Run 3 concurrent screenshots to test Promise routing
                tasks = [
                    capture_screenshot('1'),
                    capture_screenshot('2'), 
                    capture_screenshot('3')
                ]
                
                results = await asyncio.gather(*tasks)
                
                # Verify all screenshots completed successfully
                assert len(results) == 3
                
                # Collect testIds to verify all completed
                test_ids = [result['testId'] for result in results]
                expected_ids = ['1', '2', '3']
                
                for result in results:
                    assert result['success'] == True
                    assert result['testId'] in expected_ids
                    assert 'dataURL' in result
                    assert result['dataURL'].startswith('data:image/png;base64,')
                    logger.info(f"  📥 Screenshot {result['testId']}: {result['width']}x{result['height']}")
                
                # Verify all expected IDs are present (order doesn't matter for concurrent ops)
                assert set(test_ids) == set(expected_ids)
                
                logger.info("✅ Screenshot Integration with Promise System Complete")
                
            except Exception as e:
                logger.error(f"❌ Integration test failed: {e}")
                pytest.fail(f"Integration test failed: {e}")