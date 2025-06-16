/**
 * UserSelector Screenshot Test
 * Tests the UserSelector widget visual functionality using screenshots
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { promisify } from 'util';
import { exec } from 'child_process';
import path from 'path';
import fs from 'fs';

const execAsync = promisify(exec);

describe('UserSelector Screenshot Tests', () => {

  test('should capture UserSelector widget in sidebar', async () => {
    const pythonScript = `#!/usr/bin/env python3
"""
UserSelector Widget Screenshot Test
"""

import asyncio
import sys
import json
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent.parent / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_user_selector_screenshot():
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'user-selector-test',
            'agentName': 'UserSelector Screenshot Test',
            'agentType': 'ai'
        })
        
        print("📸 Testing UserSelector widget screenshot...")
        
        # Use our modular screenshot command to capture the UserSelector widget
        result = await client.screenshot({
            'selector': 'user-selector, .user-selector, [class*="user-selector"], [id*="user"]',
            'filename': 'user-selector-widget-test.png',
            'description': 'UserSelector widget functionality test'
        })
        
        if result['success']:
            print("✅ UserSelector widget screenshot captured successfully")
            print(f"Screenshot saved: {result.get('filename', 'unknown')}")
            return True
        else:
            print(f"❌ Screenshot failed: {result.get('error', 'Unknown error')}")
            return False

if __name__ == '__main__':
    result = asyncio.run(test_user_selector_screenshot())
    print('SUCCESS' if result else 'FAILED')
`;

    const projectRoot = process.cwd();
    const tempScriptPath = path.join(projectRoot, 'python-client', 'temp_user_selector_screenshot_test.py');
    
    fs.writeFileSync(tempScriptPath, pythonScript);
    
    try {
      // Execute the Python script
      const result = await execAsync(`cd ${projectRoot}/python-client && python3 temp_user_selector_screenshot_test.py`);
      
      console.log('UserSelector screenshot test result:', result.stdout);
      
      // Check if the test passed
      assert(result.stdout.includes('SUCCESS') || result.stdout.includes('✅'), 'UserSelector widget screenshot should work');
      
    } finally {
      // Clean up temp script
      if (fs.existsSync(tempScriptPath)) {
        fs.unlinkSync(tempScriptPath);
      }
    }
  });

  test('should interact with UserSelector search functionality', async () => {
    const pythonScript = `#!/usr/bin/env python3
"""
UserSelector Interaction Test
"""

import asyncio
import sys
import json
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent.parent / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_user_selector_interaction():
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'user-selector-interaction-test',
            'agentName': 'UserSelector Interaction Test',
            'agentType': 'ai'
        })
        
        print("🔍 Testing UserSelector search interaction...")
        
        # Test search input interaction
        search_result = await client.js.execute(\\`
            (function() {
                const userSelector = document.querySelector('user-selector');
                if (userSelector && userSelector.shadowRoot) {
                    const searchInput = userSelector.shadowRoot.querySelector('.search-input');
                    if (searchInput) {
                        searchInput.value = 'code';
                        searchInput.dispatchEvent(new Event('input', { bubbles: true }));
                        return { success: true, searchValue: searchInput.value };
                    }
                }
                return { success: false, error: 'UserSelector search input not found' };
            })();
        \\`)
        
        if search_result['success']:
            data = json.loads(search_result['result'])
            if data.get('success'):
                print("✅ UserSelector search interaction successful")
                return True
            else:
                print(f"❌ Search interaction failed: {data.get('error')}")
                return False
        else:
            print("❌ JavaScript execution failed")
            return False

if __name__ == '__main__':
    result = asyncio.run(test_user_selector_interaction())
    print('SUCCESS' if result else 'FAILED')
`;

    const projectRoot = process.cwd();
    const tempScriptPath = path.join(projectRoot, 'python-client', 'temp_user_selector_interaction_test.py');
    
    fs.writeFileSync(tempScriptPath, pythonScript);
    
    try {
      const result = await execAsync(`cd ${projectRoot}/python-client && python3 temp_user_selector_interaction_test.py`);
      
      console.log('UserSelector interaction test result:', result.stdout);
      assert(result.stdout.includes('SUCCESS') || result.stdout.includes('✅'), 'UserSelector interaction should work');
      
    } finally {
      if (fs.existsSync(tempScriptPath)) {
        fs.unlinkSync(tempScriptPath);
      }
    }
  });
});