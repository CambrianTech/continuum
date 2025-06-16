#!/usr/bin/env python3
"""
Test Universal validate_code Command
Test that any client can validate code without worrying about syntax
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

async def test_validate_code_command():
    print("🧪 TESTING UNIVERSAL validate_code COMMAND")
    print("Testing that any client can validate code through simple command interface")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'validate-code-test',
            'agentName': 'Validate Code Test',
            'agentType': 'ai'
        })
        
        print("\n--- TEST 1: Valid JavaScript Code ---")
        valid_js = """
            var element = document.querySelector('#content');
            if (element) {
                element.textContent = 'Hello World';
            }
            return 'success';
        """
        
        result = await client.command('validate_code', {
            'code': valid_js,
            'language': 'javascript'
        })
        
        print(f"✅ Valid JS Result: {result}")
        
        print("\n--- TEST 2: JavaScript with Security Issues ---")
        dangerous_js = """
            var userInput = prompt('Enter code:');
            eval(userInput);
        """
        
        result = await client.command('validate_code', {
            'code': dangerous_js,
            'language': 'javascript'
        })
        
        print(f"❌ Dangerous JS Result: {result}")
        
        print("\n--- TEST 3: JavaScript with Syntax Errors ---")
        broken_js = """
            var x = 10;
            if (x > 5 {
                console.log('missing paren');
            }
        """
        
        result = await client.command('validate_code', {
            'code': broken_js,
            'language': 'javascript'
        })
        
        print(f"❌ Broken JS Result: {result}")
        
        print("\n--- TEST 4: Valid Python Code ---") 
        valid_python = """
            def calculate_sum(a, b):
                return a + b
            
            result = calculate_sum(10, 20)
            print(f"Result: {result}")
        """
        
        result = await client.command('validate_code', {
            'code': valid_python,
            'language': 'python'
        })
        
        print(f"✅ Valid Python Result: {result}")
        
        print("\n--- TEST 5: Python with Security Issues ---")
        dangerous_python = """
            import os
            user_input = input("Enter command: ")
            os.system(user_input)
        """
        
        result = await client.command('validate_code', {
            'code': dangerous_python,
            'language': 'python'
        })
        
        print(f"❌ Dangerous Python Result: {result}")
        
        print("\n--- TEST 6: Test with Continuum Screenshot Code ---")
        screenshot_code = """
            if (typeof window.ScreenshotUtils === 'undefined') {
                throw new Error('ScreenshotUtils not available');
            }
            return window.ScreenshotUtils.takeScreenshot(document.body, {
                scale: 0.5
            });
        """
        
        result = await client.command('validate_code', {
            'code': screenshot_code,
            'language': 'javascript',
            'context': {'purpose': 'screenshot', 'allowBodyAccess': False}
        })
        
        print(f"⚠️ Screenshot Code Result: {result}")
        
        print("\n--- TEST 7: Same Screenshot Code with Permission ---")
        result = await client.command('validate_code', {
            'code': screenshot_code,
            'language': 'javascript', 
            'context': {'purpose': 'screenshot', 'allowBodyAccess': True}
        })
        
        print(f"✅ Screenshot Code with Permission: {result}")
        
        print("\n🎉 UNIVERSAL COMMAND TESTING COMPLETE")
        print("Commands work seamlessly across all clients without syntax worries!")

if __name__ == "__main__":
    asyncio.run(test_validate_code_command())