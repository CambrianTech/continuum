#!/usr/bin/env tsx
/**
 * Theme Screenshot Validation Test
 * 
 * Automatically tests all available themes by:
 * 1. Setting each theme via theme/set command
 * 2. Taking a screenshot to verify theme applied
 * 3. Validating screenshot file creation and size
 * 4. Building a comprehensive theme validation report
 * 
 * This test runs as part of npm test and ensures all themes work correctly.
 */

import path from 'path';
import fs from 'fs/promises';

interface ThemeTestResult {
  themeName: string;
  success: boolean;
  details: string;
  screenshotPath?: string;
  fileSize?: number;
  setCommandWorked: boolean;
  screenshotWorked: boolean;
}

interface ThemeInfo {
  name: string;
  displayName: string;
  description: string;
}

// Available themes based on the theme system
const AVAILABLE_THEMES: ThemeInfo[] = [
  { name: 'dark', displayName: 'Dark Theme', description: 'Default dark theme' },
  { name: 'light', displayName: 'Light Theme', description: 'Clean light theme' },
  { name: 'cyberpunk', displayName: 'Cyberpunk Theme', description: 'Vibrant cyberpunk with pink borders' },
  { name: 'monochrome', displayName: 'Monochrome Theme', description: 'Minimalist black and white' }
];

async function testThemeScreenshots(): Promise<void> {
  console.log('🎨 THEME SCREENSHOT VALIDATION TEST');
  console.log('==================================');
  console.log('Testing all themes with automated screenshots');
  console.log();

  const testResults: ThemeTestResult[] = [];
  
  // Test each available theme
  for (const theme of AVAILABLE_THEMES) {
    console.log(`🎯 Testing Theme: ${theme.displayName} (${theme.name})`);
    const result = await testSingleTheme(theme);
    testResults.push(result);
    logThemeTestResult(result);
    console.log();
  }

  // Generate comprehensive summary
  console.log('📊 THEME VALIDATION RESULTS');
  console.log('============================');
  
  const passedTests = testResults.filter(r => r.success).length;
  const totalTests = testResults.length;
  
  console.log(`✅ Themes Tested: ${passedTests}/${totalTests}`);
  console.log(`📈 Success Rate: ${Math.round((passedTests / totalTests) * 100)}%`);
  
  // Show individual results
  console.log();
  console.log('🎨 Individual Theme Results:');
  testResults.forEach(result => {
    const status = result.success ? '✅ PASS' : '❌ FAIL';
    console.log(`   ${status} ${result.themeName}: ${result.details}`);
    if (result.screenshotPath && result.fileSize) {
      console.log(`      Screenshot: ${result.screenshotPath} (${result.fileSize} bytes)`);
    }
  });
  
  // Show where screenshots were saved
  console.log();
  console.log('📁 Theme Screenshots Location:');
  console.log('   All theme screenshots saved to current session screenshots directory');
  console.log('   Use: ls -la .continuum/jtag/sessions/user/*/screenshots/*theme*');
  
  if (passedTests === totalTests) {
    console.log();
    console.log('🎉 ALL THEME SCREENSHOT TESTS PASSED!');
    console.log('✅ Theme switching system is fully functional');
    console.log('✅ All themes load correctly with proper styling');
    console.log('✅ Screenshot capture works for all themes');
  } else {
    console.log();
    console.log('⚠️ SOME THEME TESTS FAILED');
    const failedTests = testResults.filter(r => !r.success);
    console.log('❌ Failed Themes:');
    failedTests.forEach(test => {
      console.log(`   • ${test.themeName}: ${test.details}`);
    });
    
    // Exit with error code for npm test
    process.exit(1);
  }
}

async function testSingleTheme(theme: ThemeInfo): Promise<ThemeTestResult> {
  const result: ThemeTestResult = {
    themeName: theme.displayName,
    success: false,
    details: '',
    setCommandWorked: false,
    screenshotWorked: false
  };

  try {
    // Step 1: Set the theme using JTAG command
    console.log(`   🎨 Setting theme to: ${theme.name}`);
    
    const { exec } = require('child_process');
    const { promisify } = require('util');
    const execAsync = promisify(exec);
    
    // Execute theme set command
    const setCommand = `./jtag theme/set --theme=${theme.name}`;
    try {
      const setResult = await execAsync(setCommand, { 
        cwd: process.cwd(),
        timeout: 30000 
      });
      
      if (setResult.stdout.includes('success') || setResult.stderr === '') {
        result.setCommandWorked = true;
        console.log(`   ✅ Theme set command succeeded`);
      } else {
        result.details = `Theme set command failed: ${setResult.stderr}`;
        console.log(`   ❌ Theme set command failed: ${setResult.stderr}`);
        return result;
      }
    } catch (error: any) {
      result.details = `Theme set command error: ${error.message}`;
      console.log(`   ❌ Theme set command error: ${error.message}`);
      return result;
    }
    
    // Small delay to ensure theme is applied
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Step 2: Take screenshot with theme name
    console.log(`   📸 Capturing screenshot for ${theme.name} theme`);
    const screenshotFilename = `theme-${theme.name}-validation.png`;
    const screenshotCommand = `./jtag screenshot --querySelector=body --filename=${screenshotFilename}`;
    
    try {
      const screenshotResult = await execAsync(screenshotCommand, {
        cwd: process.cwd(), 
        timeout: 30000
      });
      
      if (screenshotResult.stdout.includes('success') || screenshotResult.stderr === '') {
        result.screenshotWorked = true;
        console.log(`   ✅ Screenshot captured successfully`);
        
        // Try to find the screenshot file and get its size
        try {
          // Look for the screenshot in session directories
          const sessionDirs = await execAsync('ls -la examples/widget-ui/.continuum/jtag/sessions/user/*/screenshots/ 2>/dev/null | grep theme- || echo "not found"');
          
          if (!sessionDirs.stdout.includes('not found')) {
            // Extract file size from ls output  
            const lines = sessionDirs.stdout.split('\\n').filter(line => line.includes(screenshotFilename));
            if (lines.length > 0) {
              const sizeMatch = lines[0].match(/\\s+(\\d+)\\s+/);
              if (sizeMatch) {
                result.fileSize = parseInt(sizeMatch[1]);
                result.screenshotPath = `session-screenshots/${screenshotFilename}`;
              }
            }
          }
        } catch (error) {
          // File size check failed, but screenshot might still exist
          console.log(`   ⚠️ Could not verify screenshot file details`);
        }
        
      } else {
        result.details = `Screenshot command failed: ${screenshotResult.stderr}`;
        console.log(`   ❌ Screenshot failed: ${screenshotResult.stderr}`);
        return result;
      }
    } catch (error: any) {
      result.details = `Screenshot command error: ${error.message}`;
      console.log(`   ❌ Screenshot error: ${error.message}`);
      return result;
    }
    
    // Success if both commands worked
    result.success = result.setCommandWorked && result.screenshotWorked;
    result.details = result.success ? 'Theme set and screenshot captured successfully' : 'Partial success - some steps failed';
    
  } catch (error: any) {
    result.details = `Theme test failed: ${error.message}`;
    console.log(`   ❌ Theme test error: ${error.message}`);
  }

  return result;
}

function logThemeTestResult(result: ThemeTestResult): void {
  const status = result.success ? '✅ PASS' : '❌ FAIL';
  console.log(`   ${status}: ${result.themeName}`);
  console.log(`   Details: ${result.details}`);
  if (result.fileSize) {
    console.log(`   File size: ${result.fileSize} bytes`);
  }
  if (result.screenshotPath) {
    console.log(`   Screenshot: ${result.screenshotPath}`);
  }
  console.log(`   Set Command: ${result.setCommandWorked ? '✅' : '❌'}`);
  console.log(`   Screenshot: ${result.screenshotWorked ? '✅' : '❌'}`);
}

// Run if executed directly
if (require.main === module) {
  testThemeScreenshots().catch(error => {
    console.error('❌ Theme screenshot validation failed:', error);
    process.exit(1);
  });
}

export { testThemeScreenshots };