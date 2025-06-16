/**
 * Base Widget Test Class
 * Shared testing functionality for all widgets
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { promisify } from 'util';
import { exec } from 'child_process';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const execAsync = promisify(exec);

export class BaseWidgetTest {
  constructor(widgetName, widgetSelector, widgetPath = '..') {
    this.widgetName = widgetName;
    this.widgetSelector = widgetSelector;
    this.widgetPath = widgetPath;
    this.widgetFilename = `${widgetName}.js`;
  }

  /**
   * Base screenshot test that every widget should have
   * Returns screenshot result for subclasses to examine
   */
  async testWidgetScreenshot(additionalSelectors = []) {
    const allSelectors = [
      this.widgetSelector,
      `.${this.widgetName.toLowerCase()}`,
      `[class*="${this.widgetName.toLowerCase()}"]`,
      `[id*="${this.widgetName.toLowerCase()}"]`,
      ...additionalSelectors
    ].join(', ');

    const pythonScript = `#!/usr/bin/env python3
"""
${this.widgetName} Widget Screenshot Test
"""

import asyncio
import sys
import json
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_${this.widgetName.toLowerCase()}_screenshot():
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': '${this.widgetName.toLowerCase()}-screenshot-test',
            'agentName': '${this.widgetName} Screenshot Test',
            'agentType': 'ai'
        })
        
        print("📸 Testing ${this.widgetName} widget screenshot...")
        
        # Use our modular screenshot command to capture the widget
        result = await client.screenshot({
            'selector': '${allSelectors}',
            'filename': '${this.widgetName.toLowerCase()}-widget-test.png',
            'description': '${this.widgetName} widget screenshot test'
        })
        
        if result['success']:
            print("✅ ${this.widgetName} widget screenshot captured successfully")
            print(f"Screenshot saved: {result.get('filename', 'unknown')}")
            print(f"Screenshot path: {result.get('filepath', 'unknown')}")
            return result
        else:
            print(f"❌ Screenshot failed: {result.get('error', 'Unknown error')}")
            return {'success': False, 'error': result.get('error', 'Unknown error')}

if __name__ == '__main__':
    import json
    result = asyncio.run(test_${this.widgetName.toLowerCase()}_screenshot())
    print('RESULT:', json.dumps(result))
`;

    // Find project root by looking for python-client directory
    let currentDir = __dirname;
    let projectRoot = null;
    
    while (currentDir !== '/') {
      if (fs.existsSync(path.join(currentDir, 'python-client'))) {
        projectRoot = currentDir;
        break;
      }
      currentDir = path.dirname(currentDir);
    }
    
    if (!projectRoot) {
      throw new Error('Could not find python-client directory in project');
    }
    
    const tempScriptPath = path.join(projectRoot, 'python-client', `temp_${this.widgetName.toLowerCase()}_screenshot_test.py`);
    
    fs.writeFileSync(tempScriptPath, pythonScript);
    
    try {
      const result = await execAsync(`cd ${projectRoot}/python-client && python3 temp_${this.widgetName.toLowerCase()}_screenshot_test.py`);
      
      console.log(`📸 ${this.widgetName} screenshot captured`);
      
      // Extract result JSON from output
      let screenshotResult = { success: false };
      const lines = result.stdout.split('\n');
      for (const line of lines) {
        if (line.startsWith('RESULT:')) {
          try {
            screenshotResult = JSON.parse(line.substring(7));
            break;
          } catch (e) {
            // Continue to next line
          }
        }
      }
      
      assert(screenshotResult.success || result.stdout.includes('✅'), `${this.widgetName} widget screenshot should work`);
      
      // Validate screenshot quality before returning
      this.validateScreenshotQuality(screenshotResult);
      
      // Store screenshot result for subclasses to examine
      this.lastScreenshotResult = {
        success: screenshotResult.success,
        filename: screenshotResult.filename || `${this.widgetName.toLowerCase()}-widget-test.png`,
        filepath: screenshotResult.filepath,
        output: result.stdout,
        selectors: allSelectors,
        width: screenshotResult.width,
        height: screenshotResult.height,
        fileSize: screenshotResult.fileSize
      };
      
      return this.lastScreenshotResult;
    } finally {
      if (fs.existsSync(tempScriptPath)) {
        fs.unlinkSync(tempScriptPath);
      }
    }
  }

  /**
   * Test WebSocket connection capability (base for all widgets)
   */
  async testWebSocketConnection() {
    const pythonScript = `#!/usr/bin/env python3
"""
Base Widget WebSocket Connection Test
"""

import asyncio
import sys
import json
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_websocket_connection():
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'base-websocket-test',
            'agentName': 'Base WebSocket Test',
            'agentType': 'ai'
        })
        
        print("🔌 Testing base WebSocket connection...")
        
        result = await client.js.execute(\`
            (function() {
                return {
                    wsConnected: window.ws && window.ws.readyState === 1,
                    wsExists: typeof window.ws !== 'undefined',
                    readyState: window.ws ? window.ws.readyState : null
                };
            })();
        \`)
        
        if result['success']:
            data = json.loads(result['result'])
            return data
        else:
            return {'success': False, 'error': 'WebSocket check failed'}

if __name__ == '__main__':
    import json
    result = asyncio.run(test_websocket_connection())
    print('RESULT:', json.dumps(result))
`;

    // Find project root by looking for python-client directory
    let currentDir = __dirname;
    let projectRoot = null;
    
    while (currentDir !== '/') {
      if (fs.existsSync(path.join(currentDir, 'python-client'))) {
        projectRoot = currentDir;
        break;
      }
      currentDir = path.dirname(currentDir);
    }
    
    if (!projectRoot) {
      throw new Error('Could not find python-client directory in project');
    }
    
    const tempScriptPath = path.join(projectRoot, 'python-client', 'temp_base_websocket_test.py');
    
    fs.writeFileSync(tempScriptPath, pythonScript);
    
    try {
      const { exec } = require('child_process');
      const { promisify } = require('util');
      const execAsync = promisify(exec);
      
      const result = await execAsync(`cd ${projectRoot}/python-client && python3 temp_base_websocket_test.py`);
      
      let wsResult = { success: false };
      const lines = result.stdout.split('\n');
      for (const line of lines) {
        if (line.startsWith('RESULT:')) {
          try {
            wsResult = JSON.parse(line.substring(7));
            break;
          } catch (e) {
            // Continue
          }
        }
      }
      
      assert(wsResult.wsExists, 'WebSocket should exist');
      console.log(`✅ WebSocket connection available (readyState: ${wsResult.readyState})`);
      
      return wsResult;
    } finally {
      if (fs.existsSync(tempScriptPath)) {
        fs.unlinkSync(tempScriptPath);
      }
    }
  }

  /**
   * Test basic widget lifecycle methods
   */
  testWidgetLifecycle() {
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, this.widgetFilename), 'utf8');
    
    // Test lifecycle methods that all widgets should have
    assert(widgetFile.includes('connectedCallback') || widgetFile.includes('initializeWidget'), 'Should have initialization');
    assert(widgetFile.includes('render'), 'Should have render method');
    assert(widgetFile.includes('setupEventListeners') || widgetFile.includes('attachEvents'), 'Should setup events');
    
    console.log(`✅ ${this.widgetName} has proper lifecycle methods`);
    return true;
  }

  /**
   * Test basic widget structure
   */
  testWidgetStructure() {
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, this.widgetFilename), 'utf8');
    
    // Test basic widget structure that all widgets should have
    assert(widgetFile.includes(`class ${this.widgetName}`), `Should have ${this.widgetName} class`);
    assert(widgetFile.includes('extends'), 'Should extend a base class');
    assert(widgetFile.includes(`this.widgetName = '${this.widgetName}'`), 'Should set widget name');
    assert(widgetFile.includes('this.widgetIcon'), 'Should set widget icon');
    
    console.log(`✅ ${this.widgetName} has proper structure`);
    return true;
  }

  /**
   * Test module exports (all widgets should export properly)
   */
  testWidgetExports() {
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, this.widgetFilename), 'utf8');
    
    assert(widgetFile.includes('customElements.define'), 'Should register custom element');
    assert(widgetFile.includes('module.exports'), 'Should export for CommonJS');
    
    console.log(`✅ ${this.widgetName} exports properly`);
    return true;
  }

  /**
   * Test package.json structure (all widgets should have proper metadata)
   */
  testWidgetPackage() {
    const packagePath = path.resolve(__dirname, this.widgetPath, 'package.json');
    assert(fs.existsSync(packagePath), 'Package.json should exist');
    
    const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
    assert.strictEqual(packageData.continuum.widgetName, this.widgetName);
    assert(packageData.continuum.icon, 'Should have widget icon in package.json');
    assert(Array.isArray(packageData.continuum.selectors), 'Should have selectors array');
    
    console.log(`✅ ${this.widgetName} package.json is valid`);
    return true;
  }

  /**
   * Test error handling capability (all widgets should handle errors gracefully)
   */
  testErrorHandling() {
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, this.widgetFilename), 'utf8');
    
    // Check for basic error handling patterns
    const hasErrorHandling = widgetFile.includes('try') && widgetFile.includes('catch') ||
                           widgetFile.includes('setError') ||
                           widgetFile.includes('hasError') ||
                           widgetFile.includes('errorMessage');
    
    assert(hasErrorHandling, 'Should have error handling mechanisms');
    
    console.log(`✅ ${this.widgetName} has error handling`);
    return true;
  }

  /**
   * Validate screenshot quality (not blank, proper size, etc.)
   */
  validateScreenshotQuality(screenshotResult) {
    // Check dimensions - should not be tiny
    if (screenshotResult.width && screenshotResult.width < 50) {
      throw new Error(`Screenshot too narrow: ${screenshotResult.width}px width`);
    }
    if (screenshotResult.height && screenshotResult.height < 50) {
      throw new Error(`Screenshot too short: ${screenshotResult.height}px height`);
    }
    
    // Check file size - should not be suspiciously small (blank image)
    if (screenshotResult.fileSize && screenshotResult.fileSize < 1000) {
      throw new Error(`Screenshot file too small: ${screenshotResult.fileSize} bytes (likely blank)`);
    }
    
    console.log(`✅ Screenshot quality validated: ${screenshotResult.width}x${screenshotResult.height}, ${screenshotResult.fileSize} bytes`);
  }

  /**
   * Run all base tests
   */
  async runBaseTests() {
    console.log(`\n🧪 Running base tests for ${this.widgetName}...`);
    
    // Test basic structure and exports
    this.testWidgetStructure();
    this.testWidgetLifecycle();
    this.testWidgetExports();
    this.testWidgetPackage();
    this.testErrorHandling();
    
    // Test runtime functionality
    await this.testWebSocketConnection();
    await this.testWidgetScreenshot();
    
    console.log(`✅ All base tests passed for ${this.widgetName}`);
  }
}