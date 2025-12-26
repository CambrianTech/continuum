#!/usr/bin/env tsx
/**
 * Simple Theme Screenshot Capture
 * 
 * Captures screenshots of all UI elements without trying to automate theme switching.
 * This provides visual documentation of the current theme system state.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import * as fs from 'fs';

const execAsync = promisify(exec);

interface ScreenshotTest {
  name: string;
  selector: string;
  expectedMinSizeKB: number;
}

const SCREENSHOT_TESTS: ScreenshotTest[] = [
  {
    name: 'Full-Page',
    selector: 'body',
    expectedMinSizeKB: 50
  },
  {
    name: 'Sidebar',
    selector: 'continuum-sidebar',
    expectedMinSizeKB: 10
  },
  {
    name: 'Chat-Widget',
    selector: 'chat-widget',
    expectedMinSizeKB: 10
  },
  {
    name: 'Main-Panel',
    selector: 'continuum-main',
    expectedMinSizeKB: 10
  }
];

async function captureCurrentThemeScreenshots(): Promise<void> {
  console.log('📸 Theme Screenshot Capture - Current State');
  console.log('=' .repeat(50));
  console.log(`🎯 Capturing ${SCREENSHOT_TESTS.length} different UI elements...\n`);
  
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5);
  const results: Array<{name: string, success: boolean, message: string, filePath?: string}> = [];
  
  for (const test of SCREENSHOT_TESTS) {
    const filename = `current-theme-${test.name.toLowerCase()}-${timestamp}.png`;
    console.log(`📸 Capturing ${test.name} (${test.selector})...`);
    
    const result = await captureScreenshot(test, filename);
    results.push(result);
    
    if (result.success) {
      console.log(`  ✅ ${result.message}`);
    } else {
      console.log(`  ❌ ${result.message}`);
    }
    
    // Small delay between captures
    await sleep(500);
  }
  
  // Generate report
  const successful = results.filter(r => r.success);
  const failed = results.filter(r => !r.success);
  
  console.log('\n' + '='.repeat(50));
  console.log('📊 SCREENSHOT CAPTURE REPORT');
  console.log('=' .repeat(50));
  console.log(`Total captures: ${results.length}`);
  console.log(`Successful: ${successful.length}`);
  console.log(`Failed: ${failed.length}`);
  console.log(`Success rate: ${((successful.length / results.length) * 100).toFixed(1)}%`);
  
  if (successful.length > 0) {
    console.log('\n📁 CAPTURED SCREENSHOTS:');
    successful.forEach(result => {
      if (result.filePath) {
        console.log(`  • ${result.name}: ${result.filePath.split('/').pop()}`);
      }
    });
    
    if (successful[0]?.filePath) {
      const directory = successful[0].filePath.replace(/[^/]*$/, '');
      console.log(`\n📂 Location: ${directory}`);
    }
  }
  
  if (failed.length > 0) {
    console.log('\n❌ FAILED CAPTURES:');
    failed.forEach(result => {
      console.log(`  • ${result.name}: ${result.message}`);
    });
  }
  
  console.log('\n' + '='.repeat(50));
}

async function captureScreenshot(test: ScreenshotTest, filename: string): Promise<{name: string, success: boolean, message: string, filePath?: string}> {
  const startTime = Date.now();
  
  try {
    const { stdout } = await execAsync(`./jtag interface/screenshot --querySelector="${test.selector}" --filename="${filename}"`);
    const executionTime = Date.now() - startTime;
    
    // Parse output to find filepath
    let filePath: string | undefined;
    const lines = stdout.split('\n');
    
    for (const line of lines) {
      if (line.includes('"filepath":')) {
        const match = line.match(/"filepath":\s*"([^"]+)"/);
        if (match) {
          filePath = match[1];
          break;
        }
      }
    }
    
    if (!filePath) {
      return {
        name: test.name,
        success: false,
        message: `Filepath not found in output (${executionTime}ms)`
      };
    }
    
    if (!fs.existsSync(filePath)) {
      return {
        name: test.name,
        success: false,
        message: `Screenshot file not created (${executionTime}ms)`
      };
    }
    
    const stats = fs.statSync(filePath);
    const fileSizeKB = stats.size / 1024;
    
    if (fileSizeKB < test.expectedMinSizeKB) {
      return {
        name: test.name,
        success: false,
        message: `Screenshot too small: ${fileSizeKB.toFixed(1)}KB < ${test.expectedMinSizeKB}KB (${executionTime}ms)`,
        filePath
      };
    }
    
    return {
      name: test.name,
      success: true,
      message: `Captured ${fileSizeKB.toFixed(1)}KB in ${executionTime}ms`,
      filePath
    };
    
  } catch (error) {
    const executionTime = Date.now() - startTime;
    return {
      name: test.name,
      success: false,
      message: `Command failed: ${error instanceof Error ? error.message : String(error)} (${executionTime}ms)`
    };
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Run if called directly
if (require.main === module) {
  captureCurrentThemeScreenshots().catch(console.error);
}

export { captureCurrentThemeScreenshots };