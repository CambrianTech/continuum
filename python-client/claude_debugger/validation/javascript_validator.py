"""
JavaScript Validation System
============================

Handles browser JavaScript execution testing and validation.
"""

import asyncio
import base64


class JavaScriptValidator:
    """
    Browser JavaScript Execution Validation
    
    Tests JavaScript execution capabilities including:
    - Code execution in browser context
    - Console message capture
    - Return value handling
    - Version detection from DOM elements
    """
    
    def __init__(self, connection):
        self.connection = connection
        
    async def test_execution(self):
        """Test JavaScript execution with version detection"""
        js_code = '''
        console.log("🔧 Claude JavaScript validation");
        const versionBadge = document.querySelector(".version-badge");
        if (versionBadge) {
            console.log("✅ Version found:", versionBadge.textContent);
            return "VERSION_" + versionBadge.textContent.trim();
        }
        return "NO_VERSION_BADGE";
        '''
        
        try:
            encoded_js = base64.b64encode(js_code.encode()).decode()
            task = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            await self.connection.send_message(task)
            
            for attempt in range(5):
                try:
                    result = await self.connection.receive_message(timeout=2)
                    
                    if result.get('type') == 'working':
                        continue
                    elif result.get('type') == 'result':
                        return self._process_browser_response(result)
                        
                except asyncio.TimeoutError:
                    continue
                    
            print(f"   ❌ JavaScript execution: TIMEOUT")
            return False
            
        except Exception as e:
            print(f"   ❌ JavaScript execution failed: {e}")
            return False
            
    def _process_browser_response(self, result):
        """Process browser response from JavaScript execution"""
        try:
            data = result.get('data', {})
            
            if 'result' in data and 'browserResponse' in data.get('result', {}):
                browser_response = data['result']['browserResponse']
                
                if browser_response.get('success'):
                    console_output = browser_response.get('output', [])
                    return_value = browser_response.get('result')
                    
                    print(f"   ✅ Console messages captured: {len(console_output)}")
                    
                    if return_value and 'VERSION_' in return_value:
                        version = return_value.split('_')[1] if '_' in return_value else 'unknown'
                        print(f"   ✅ Version detected: {version}")
                        
                    return True
                else:
                    print(f"   ❌ Browser execution failed")
                    return False
            else:
                print(f"   ⚠️ No browser response received")
                return False
                
        except Exception as e:
            print(f"   ❌ Response processing failed: {e}")
            return False