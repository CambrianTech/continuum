"""
Screenshot Management System
===========================

Handles screenshot capture, validation, and file management.
"""

import asyncio


class ScreenshotManager:
    """
    Screenshot Capture and Validation Manager
    
    Handles all screenshot-related operations including:
    - Screenshot command execution
    - Image capture validation
    - File saving coordination
    - Cross-client screenshot sharing
    """
    
    def __init__(self, connection):
        self.connection = connection
        self.last_screenshot = None
        self.screenshot_history = []
        
    async def test_screenshot(self):
        """Test screenshot capability"""
        try:
            screenshot_task = {
                'type': 'task',
                'role': 'system',
                'task': '[CMD:SCREENSHOT] {}'
            }
            
            await self.connection.send_message(screenshot_task)
            
            for attempt in range(5):
                try:
                    result = await self.connection.receive_message(timeout=5)
                    
                    if result.get('type') == 'working':
                        continue
                    elif result.get('type') == 'result':
                        print(f"📸 Screenshot result received")
                        return True
                        
                except asyncio.TimeoutError:
                    continue
                    
            return False
            
        except Exception as e:
            print(f"❌ Screenshot test failed: {e}")
            return False
            
    async def capture_element_screenshot(self, selector):
        """Capture screenshot of specific element"""
        try:
            screenshot_task = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:SCREENSHOT] {{"selector": "{selector}"}}'
            }
            
            await self.connection.send_message(screenshot_task)
            
            # Wait for result and process
            for attempt in range(5):
                try:
                    result = await self.connection.receive_message(timeout=5)
                    
                    if result.get('type') == 'working':
                        continue
                    elif result.get('type') == 'result':
                        # Process screenshot result
                        self.last_screenshot = {
                            'selector': selector,
                            'result': result,
                            'timestamp': asyncio.get_event_loop().time()
                        }
                        self.screenshot_history.append(self.last_screenshot)
                        return True
                        
                except asyncio.TimeoutError:
                    continue
                    
            return False
            
        except Exception as e:
            print(f"❌ Element screenshot failed: {e}")
            return False