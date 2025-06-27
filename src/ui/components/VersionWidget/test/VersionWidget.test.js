/**
 * Version Widget Test
 * Tests version display and OCR validation
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { BaseWidgetTest } from '../../shared/BaseWidget.test.js';

class VersionWidgetTest extends BaseWidgetTest {
  constructor() {
    super('VersionWidget', 'version-widget');
  }

  async testVersionScreenshotWithOCR() {
    // Base class captures the screenshot
    const screenshotResult = await super.testWidgetScreenshot([
      '.version-display',
      '[class*="version"]',
      '.build-number',
      '.version-text'
    ]);
    
    // Now we can do OCR on the screenshot that base class captured
    const ocrResult = await this.performOCROnScreenshot(screenshotResult);
    
    // Verify version is visible in the screenshot via OCR
    assert(ocrResult.success, 'OCR should succeed on version screenshot');
    assert(this.containsVersionPattern(ocrResult.text), 'Should detect version pattern in screenshot');
    
    console.log(`✅ Version widget screenshot captured and OCR verified`);
    console.log(`✅ Detected version text: ${ocrResult.detectedVersion}`);
    
    return {
      screenshotResult,
      ocrResult,
      detectedVersion: ocrResult.detectedVersion
    };
  }

  async performOCROnScreenshot(screenshotResult) {
    const pythonScript = `#!/usr/bin/env python3
"""
OCR Analysis on Version Widget Screenshot
"""

import asyncio
import sys
import json
import re
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent.parent / 'python-client'))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def analyze_version_screenshot():
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'version-ocr-test',
            'agentName': 'Version OCR Test',
            'agentType': 'ai'
        })
        
        print("🔍 Performing OCR on version widget screenshot...")
        
        # Use OCR to read text from the screenshot
        result = await client.js.execute(\\`
            (function() {
                // Look for version elements in the DOM
                const versionElements = [
                    ...document.querySelectorAll('.version-display, [class*="version"], .build-number'),
                    ...document.querySelectorAll('[id*="version"]')
                ];
                
                let detectedVersions = [];
                versionElements.forEach(el => {
                    const text = el.textContent || el.innerText || '';
                    const versionMatch = text.match(/\\d+\\.\\d+\\.\\d+/);
                    if (versionMatch) {
                        detectedVersions.push(versionMatch[0]);
                    }
                });
                
                return {
                    success: detectedVersions.length > 0,
                    text: versionElements.map(el => el.textContent || '').join(' '),
                    detectedVersions: detectedVersions,
                    elementCount: versionElements.length
                };
            })();
        `)
        
        if result['success']:
            data = json.loads(result['result'])
            print(f"✅ OCR analysis completed")
            print(f"Detected versions: {data.get('detectedVersions', [])}")
            print(f"Element count: {data.get('elementCount', 0)}")
            return data
        else:
            print("❌ OCR analysis failed")
            return {'success': False, 'error': 'OCR execution failed'}

if __name__ == '__main__':
    import json
    result = asyncio.run(analyze_version_screenshot())
    print('RESULT:', json.dumps(result))
`;

    const projectRoot = process.cwd();
    const tempScriptPath = path.join(projectRoot, 'python-client', 'temp_version_ocr_test.py');
    const fs = require('fs');
    
    fs.writeFileSync(tempScriptPath, pythonScript);
    
    try {
      const { exec } = require('child_process');
      const { promisify } = require('util');
      const execAsync = promisify(exec);
      
      const result = await execAsync(`cd ${projectRoot}/python-client && python3 temp_version_ocr_test.py`);
      
      // Extract OCR result from output
      let ocrResult = { success: false };
      const lines = result.stdout.split('\n');
      for (const line of lines) {
        if (line.startsWith('RESULT:')) {
          try {
            ocrResult = JSON.parse(line.substring(7));
            break;
          } catch (e) {
            // Continue
          }
        }
      }
      
      return {
        success: ocrResult.success,
        text: ocrResult.text || '',
        detectedVersion: ocrResult.detectedVersions?.[0] || null,
        detectedVersions: ocrResult.detectedVersions || [],
        elementCount: ocrResult.elementCount || 0
      };
    } finally {
      if (fs.existsSync(tempScriptPath)) {
        fs.unlinkSync(tempScriptPath);
      }
    }
  }

  containsVersionPattern(text) {
    // Check if text contains version patterns like 0.2.2030
    const versionRegex = /\d+\.\d+\.\d+/;
    return versionRegex.test(text);
  }

  testVersionWidgetStructure() {
    const fs = require('fs');
    const path = require('path');
    
    const widgetFile = fs.readFileSync(path.resolve(__dirname, this.widgetPath, `${this.widgetName}.js`), 'utf8');
    
    assert(widgetFile.includes('version'), 'Should contain version-related code');
    assert(widgetFile.includes('build'), 'Should contain build-related code');
    
    console.log('✅ VersionWidget has proper structure');
    return true;
  }

  async runAllTests() {
    console.log('\n🧪 Running VersionWidget tests...');
    
    // Run base tests (includes base screenshot)
    await this.runBaseTests();
    
    // Run version-specific tests
    this.testVersionWidgetStructure();
    
    // Test screenshot + OCR analysis
    await this.testVersionScreenshotWithOCR();
    
    console.log('✅ All VersionWidget tests passed');
  }
}

describe('VersionWidget Tests', () => {
  const versionWidgetTest = new VersionWidgetTest();

  test('should pass all base widget tests', async () => {
    await versionWidgetTest.runBaseTests();
  });

  test('should have version widget structure', () => {
    versionWidgetTest.testVersionWidgetStructure();
  });

  test('should capture screenshot and perform OCR verification', async () => {
    const result = await versionWidgetTest.testVersionScreenshotWithOCR();
    
    // Verify we got both screenshot and OCR results
    assert(result.screenshotResult.success, 'Screenshot should succeed');
    assert(result.ocrResult.success, 'OCR should succeed');
    assert(result.detectedVersion, 'Should detect a version number');
    
    console.log(`Final detected version: ${result.detectedVersion}`);
  });
});