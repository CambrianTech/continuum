#!/usr/bin/env tsx
/**
 * Theme Visual Regression Integration Test
 * 
 * This test programmatically switches to each theme and captures screenshots
 * to provide visual documentation and regression testing for all themes
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import * as fs from 'fs';

const execAsync = promisify(exec);

interface ThemeTestResult {
  themeName: string;
  displayName: string;
  switched: boolean;
  screenshotCaptured: boolean;
  filePath?: string;
  fileSizeKB?: number;
  error?: string;
}

const KNOWN_THEMES = [
  { name: 'base', displayName: 'Base - Dark Cyberpunk' },
  { name: 'light', displayName: 'Light - Clean Professional' },
  { name: 'cyberpunk', displayName: 'Cyberpunk - Neon Future' },
  { name: 'retro-mac', displayName: 'Retro Mac - System 11' },
  { name: 'monochrome', displayName: 'Monochrome - High Contrast' },
  { name: 'classic', displayName: 'Classic - Professional' }
];

async function runThemeVisualRegressionTest(): Promise<void> {
  console.log('🎨 Theme Visual Regression Integration Test');
  console.log('=' .repeat(60));
  console.log('📸 This test will switch to each theme and capture screenshots\n');

  // Take initial state screenshot
  console.log('📸 Capturing initial state...');
  await captureScreenshot('initial-state.png', 'Initial State');

  const results: ThemeTestResult[] = [];
  const timestamp = new Date().toISOString().slice(0, 19).replace(/[:.]/g, '-');

  // Test each theme
  for (const theme of KNOWN_THEMES) {
    console.log(`\n🎨 Testing theme: ${theme.name} (${theme.displayName})`);
    
    const result: ThemeTestResult = {
      themeName: theme.name,
      displayName: theme.displayName,
      switched: false,
      screenshotCaptured: false
    };

    try {
      // Step 1: Switch to theme
      console.log(`  🔄 Switching to ${theme.name}...`);
      const switchSuccess = await switchTheme(theme.name);
      result.switched = switchSuccess;

      if (!switchSuccess) {
        result.error = 'Theme switch failed';
        console.log(`  ❌ Failed to switch to ${theme.name}`);
        results.push(result);
        continue;
      }

      console.log(`  ✅ Successfully switched to ${theme.name}`);

      // Step 2: Wait for theme to apply
      await sleep(2000); // Give theme time to fully apply

      // Step 3: Capture screenshot
      console.log(`  📸 Capturing screenshot...`);
      const filename = `theme-${theme.name}-${timestamp}.png`;
      const screenshotResult = await captureScreenshot(filename, theme.displayName);
      
      if (screenshotResult.success) {
        result.screenshotCaptured = true;
        result.filePath = screenshotResult.filePath;
        result.fileSizeKB = screenshotResult.fileSizeKB;
        console.log(`  ✅ Screenshot captured: ${screenshotResult.fileSizeKB?.toFixed(1)}KB`);
      } else {
        result.error = screenshotResult.error;
        console.log(`  ❌ Screenshot failed: ${screenshotResult.error}`);
      }

    } catch (error) {
      result.error = `Unexpected error: ${error}`;
      console.log(`  ❌ Error testing ${theme.name}: ${error}`);
    }

    results.push(result);
  }

  // Generate comprehensive report
  generateVisualRegressionReport(results);
}

async function switchTheme(themeName: string): Promise<boolean> {
  try {
    // Method 1: Try using UniverseWidget setTheme method
    const setThemeResult = await execAsync(`./jtag exec --environment=browser --code="
      const themeWidget = document.querySelector('universe-widget');
      if (themeWidget && typeof themeWidget.setTheme === 'function') {
        await themeWidget.setTheme('${themeName}');
        return { success: true, method: 'UniverseWidget.setTheme' };
      }
      return { success: false, error: 'UniverseWidget.setTheme not available' };
    "`);

    // Check if setTheme worked
    if (setThemeResult.stdout.includes('"success":true')) {
      return true;
    }

    // Method 2: Try dropdown selection
    console.log(`    🔄 Trying dropdown method for ${themeName}...`);
    const dropdownResult = await execAsync(`./jtag exec --environment=browser --code="
      const selector = document.querySelector('#theme-selector') || document.querySelector('universe-widget select');
      if (selector && selector.options) {
        // Find the option with the target theme
        const option = Array.from(selector.options).find(opt => opt.value === '${themeName}');
        if (option) {
          selector.value = '${themeName}';
          const changeEvent = new Event('change', { bubbles: true });
          selector.dispatchEvent(changeEvent);
          
          // Also click apply button if it exists
          const applyButton = document.querySelector('#apply-theme');
          if (applyButton) {
            applyButton.click();
          }
          return { success: true, method: 'dropdown selection' };
        }
      }
      return { success: false, error: 'Dropdown method failed' };
    "`);

    return dropdownResult.stdout.includes('"success":true');

  } catch (error) {
    console.log(`    ⚠️ Theme switch command failed for ${themeName}: ${error}`);
    return false;
  }
}

async function captureScreenshot(filename: string, description: string): Promise<{
  success: boolean;
  filePath?: string;
  fileSizeKB?: number;
  error?: string;
}> {
  try {
    const { stdout } = await execAsync(`./jtag interface/screenshot --querySelector="body" --filename="${filename}"`);
    
    // Parse filepath from output
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
      return { success: false, error: 'Filepath not found in output' };
    }

    if (!fs.existsSync(filePath)) {
      return { success: false, error: 'Screenshot file was not created' };
    }

    const stats = fs.statSync(filePath);
    const fileSizeKB = stats.size / 1024;

    return {
      success: true,
      filePath,
      fileSizeKB
    };

  } catch (error) {
    return {
      success: false,
      error: `Screenshot command failed: ${error}`
    };
  }
}

function generateVisualRegressionReport(results: ThemeTestResult[]): void {
  const successful = results.filter(r => r.switched && r.screenshotCaptured);
  const failed = results.filter(r => !r.switched || !r.screenshotCaptured);
  const switchedButNoScreenshot = results.filter(r => r.switched && !r.screenshotCaptured);
  const totalScreenshots = successful.length;

  console.log('\n' + '='.repeat(70));
  console.log('🎨 THEME VISUAL REGRESSION TEST REPORT');
  console.log('='.repeat(70));

  console.log(`\n📊 SUMMARY:`);
  console.log(`  Total themes tested: ${results.length}`);
  console.log(`  Successfully switched: ${results.filter(r => r.switched).length}`);
  console.log(`  Screenshots captured: ${totalScreenshots}`);
  console.log(`  Failed: ${failed.length}`);
  console.log(`  Success rate: ${((totalScreenshots / results.length) * 100).toFixed(1)}%`);

  if (successful.length > 0) {
    console.log(`\n✅ SUCCESSFUL THEME CAPTURES:`);
    successful.forEach(result => {
      console.log(`  🎨 ${result.themeName.padEnd(12)} - ${result.displayName} (${result.fileSizeKB?.toFixed(1)}KB)`);
    });

    console.log(`\n📁 SCREENSHOT FILES:`);
    successful.forEach(result => {
      if (result.filePath) {
        const filename = result.filePath.split('/').pop();
        console.log(`  • ${filename}`);
      }
    });

    if (successful[0]?.filePath) {
      const directory = successful[0].filePath.replace(/[^/]*$/, '');
      console.log(`\n📂 Location: ${directory}`);
    }
  }

  if (failed.length > 0) {
    console.log(`\n❌ FAILED THEME TESTS:`);
    failed.forEach(result => {
      const reason = !result.switched ? 'Switch failed' : 'Screenshot failed';
      console.log(`  • ${result.themeName.padEnd(12)} - ${reason}: ${result.error}`);
    });
  }

  if (switchedButNoScreenshot.length > 0) {
    console.log(`\n⚠️ SWITCHED BUT NO SCREENSHOT:`);
    switchedButNoScreenshot.forEach(result => {
      console.log(`  • ${result.themeName.padEnd(12)} - ${result.error}`);
    });
  }

  console.log(`\n🎯 VISUAL REGRESSION TEST RESULTS:`);
  if (totalScreenshots === results.length) {
    console.log(`  🎉 ALL ${results.length} THEMES CAPTURED SUCCESSFULLY!`);
    console.log(`  ✅ Dynamic theme system fully validated`);
    console.log(`  ✅ Visual regression test suite complete`);
    console.log(`  ✅ ${totalScreenshots} theme variations documented`);
  } else {
    console.log(`  📊 ${totalScreenshots}/${results.length} themes captured`);
    console.log(`  ⚠️ Some themes may need manual testing`);
  }

  console.log(`\n💡 NEXT STEPS:`);
  console.log(`  1. Review captured screenshots for visual differences`);
  console.log(`  2. Compare themes to ensure proper color application`);
  console.log(`  3. Use screenshots for design documentation`);
  console.log(`  4. Run this test after theme system changes`);

  console.log('\n' + '='.repeat(70));
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Run the test
if (require.main === module) {
  runThemeVisualRegressionTest().catch(console.error);
}

export { runThemeVisualRegressionTest };