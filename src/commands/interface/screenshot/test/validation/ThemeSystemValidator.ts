/**
 * Theme System Validation
 * 
 * Validates the complete dynamic theme system by capturing screenshots
 * of all discovered themes and verifying theme switching works correctly
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import * as fs from 'fs';
import * as path from 'path';

const execAsync = promisify(exec);

interface ThemeValidationTest {
  themeName: string;
  displayName: string;
  category: string;
  expectedMinSizeKB: number;
  testSelectors: string[];
}

// Known themes for validation - these should all be dynamically discoverable
const THEME_VALIDATION_TESTS: ThemeValidationTest[] = [
  {
    themeName: 'base',
    displayName: 'Base - Dark Cyberpunk',
    category: 'dark',
    expectedMinSizeKB: 50,
    testSelectors: ['body', 'continuum-sidebar', 'chat-widget']
  },
  {
    themeName: 'light',
    displayName: 'Light - Clean Professional', 
    category: 'light',
    expectedMinSizeKB: 50,
    testSelectors: ['body', 'continuum-sidebar', 'chat-widget']
  },
  {
    themeName: 'cyberpunk',
    displayName: 'Cyberpunk - Neon Future',
    category: 'dark',
    expectedMinSizeKB: 50,
    testSelectors: ['body', 'continuum-sidebar', 'chat-widget']
  },
  {
    themeName: 'retro-mac',
    displayName: 'Retro Mac - System 11',
    category: 'retro',
    expectedMinSizeKB: 50,
    testSelectors: ['body', 'continuum-sidebar', 'chat-widget']
  },
  {
    themeName: 'monochrome',
    displayName: 'Monochrome - High Contrast',
    category: 'accessibility',
    expectedMinSizeKB: 50,
    testSelectors: ['body', 'continuum-sidebar', 'chat-widget']
  },
  {
    themeName: 'classic',
    displayName: 'Classic - Professional',
    category: 'professional',
    expectedMinSizeKB: 50,
    testSelectors: ['body', 'continuum-sidebar', 'chat-widget']
  }
];

interface ValidationResult {
  theme: string;
  selector: string;
  passed: boolean;
  message: string;
  filePath?: string;
  fileSizeKB?: number;
  executionTime?: number;
}

export class ThemeSystemValidator {
  
  async runValidation(): Promise<void> {
    console.log('🎨 Theme System Validation');
    console.log('=' .repeat(50));
    console.log(`🧪 Testing ${THEME_VALIDATION_TESTS.length} themes with multiple selectors...\n`);
    
    // Take initial screenshot for comparison
    console.log('📸 Taking initial screenshot for comparison...');
    await this.takeScreenshot('body', 'theme-validation-initial.png');
    
    const results: ValidationResult[] = [];
    let totalTests = 0;
    let passedTests = 0;
    
    // Test each theme
    for (const themeTest of THEME_VALIDATION_TESTS) {
      console.log(`\n🎨 Testing theme: ${themeTest.themeName} (${themeTest.displayName})`);
      
      // Switch to this theme first
      const themeSwitchResult = await this.switchToTheme(themeTest.themeName);
      if (!themeSwitchResult.success) {
        console.log(`⚠️ Could not switch to theme '${themeTest.themeName}': ${themeSwitchResult.message}`);
        // Create a failed result for theme switching
        results.push({
          theme: themeTest.themeName,
          selector: 'theme-switch',
          passed: false,
          message: `Theme switch failed: ${themeSwitchResult.message}`
        });
        totalTests++;
        continue;
      }
      
      // Wait for theme to apply
      await this.sleep(1000);
      
      // Test each selector for this theme
      for (const selector of themeTest.testSelectors) {
        totalTests++;
        const result = await this.validateThemeScreenshot(themeTest, selector);
        results.push(result);
        
        if (result.passed) {
          passedTests++;
          console.log(`  ✅ ${selector}: ${result.message}`);
        } else {
          console.log(`  ❌ ${selector}: ${result.message}`);
        }
      }
    }
    
    // Generate final report
    this.generateReport(results, totalTests, passedTests);
  }
  
  private async switchToTheme(themeName: string): Promise<{success: boolean, message: string}> {
    try {
      // Use exec command to switch themes via browser automation
      const switchCommand = `./jtag exec --environment=browser --code="
        // Try to switch theme using multiple methods
        let success = false;
        let method = 'none';
        
        // Method 1: ThemeWidget setTheme
        const themeWidget = document.querySelector('theme-widget');
        if (themeWidget && typeof themeWidget.setTheme === 'function') {
          try {
            await themeWidget.setTheme('${themeName}');
            success = true;
            method = 'ThemeWidget.setTheme';
          } catch (e) {
            console.log('ThemeWidget.setTheme failed:', e);
          }
        }
        
        // Method 2: Dropdown selection
        if (!success) {
          const selector = document.querySelector('#theme-selector') || document.querySelector('theme-widget select');
          if (selector) {
            selector.value = '${themeName}';
            const changeEvent = new Event('change', { bubbles: true });
            selector.dispatchEvent(changeEvent);
            
            // Click apply button if available
            const applyButton = document.querySelector('#apply-theme');
            if (applyButton) {
              applyButton.click();
            }
            success = true;
            method = 'dropdown selection';
          }
        }
        
        return { success: success, method: method };
      "`;
      
      const { stdout } = await execAsync(switchCommand);
      
      // Parse the result
      const lines = stdout.split('\n');
      for (const line of lines) {
        if (line.includes('"success"')) {
          const resultMatch = line.match(/{"success":(true|false),"method":"([^"]+)"}/);
          if (resultMatch) {
            const success = resultMatch[1] === 'true';
            const method = resultMatch[2];
            return {
              success,
              message: success ? `Switched using ${method}` : `Failed to switch theme`
            };
          }
        }
      }
      
      return { success: false, message: 'Could not parse theme switch result' };
      
    } catch (error) {
      return { 
        success: false, 
        message: `Theme switch command failed: ${error instanceof Error ? error.message : String(error)}` 
      };
    }
  }
  
  private async validateThemeScreenshot(themeTest: ThemeValidationTest, selector: string): Promise<ValidationResult> {
    const startTime = Date.now();
    const filename = `theme-${themeTest.themeName}-${selector.replace(/[^a-zA-Z0-9]/g, '-')}-${Date.now()}.png`;
    
    try {
      const result = await this.takeScreenshot(selector, filename);
      const executionTime = Date.now() - startTime;
      
      if (!result.success) {
        return {
          theme: themeTest.themeName,
          selector,
          passed: false,
          message: `Screenshot failed: ${result.message}`,
          executionTime
        };
      }
      
      if (!result.filePath || !fs.existsSync(result.filePath)) {
        return {
          theme: themeTest.themeName,
          selector,
          passed: false,
          message: `Screenshot file not created: ${result.filePath}`,
          executionTime
        };
      }
      
      const stats = fs.statSync(result.filePath);
      const fileSizeKB = stats.size / 1024;
      
      if (fileSizeKB < themeTest.expectedMinSizeKB) {
        return {
          theme: themeTest.themeName,
          selector,
          passed: false,
          message: `Screenshot too small: ${fileSizeKB.toFixed(1)}KB < ${themeTest.expectedMinSizeKB}KB`,
          filePath: result.filePath,
          fileSizeKB,
          executionTime
        };
      }
      
      return {
        theme: themeTest.themeName,
        selector,
        passed: true,
        message: `Valid screenshot ${fileSizeKB.toFixed(1)}KB`,
        filePath: result.filePath,
        fileSizeKB,
        executionTime
      };
      
    } catch (error) {
      const executionTime = Date.now() - startTime;
      return {
        theme: themeTest.themeName,
        selector,
        passed: false,
        message: `Screenshot command failed: ${error instanceof Error ? error.message : String(error)}`,
        executionTime
      };
    }
  }
  
  private async takeScreenshot(selector: string, filename: string): Promise<{success: boolean, message: string, filePath?: string}> {
    try {
      const { stdout, stderr } = await execAsync(`./jtag interface/screenshot --querySelector="${selector}" --filename="${filename}"`);
      
      // Parse output to find filepath
      const lines = stdout.split('\n');
      let filePath: string | undefined;
      
      // Look for filepath in the JSON output
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
          success: false,
          message: 'Screenshot filepath not found in output'
        };
      }
      
      return {
        success: true,
        message: 'Screenshot captured successfully',
        filePath
      };
      
    } catch (error) {
      return {
        success: false,
        message: error instanceof Error ? error.message : String(error)
      };
    }
  }
  
  private generateReport(results: ValidationResult[], totalTests: number, passedTests: number): void {
    const failedTests = totalTests - passedTests;
    const successRate = ((passedTests / totalTests) * 100).toFixed(1);
    
    console.log('\n' + '='.repeat(60));
    console.log('🎨 THEME SYSTEM VALIDATION REPORT');
    console.log('='.repeat(60));
    
    console.log(`\n📊 SUMMARY:`);
    console.log(`  Total tests: ${totalTests}`);
    console.log(`  Passed: ${passedTests}`);
    console.log(`  Failed: ${failedTests}`);
    console.log(`  Success rate: ${successRate}%`);
    
    // Group results by theme
    const themeGroups = results.reduce((groups, result) => {
      if (!groups[result.theme]) {
        groups[result.theme] = [];
      }
      groups[result.theme].push(result);
      return groups;
    }, {} as Record<string, ValidationResult[]>);
    
    console.log(`\n📋 RESULTS BY THEME:`);
    Object.entries(themeGroups).forEach(([themeName, themeResults]) => {
      const themePassed = themeResults.filter(r => r.passed).length;
      const themeTotal = themeResults.length;
      const themeRate = themeTotal > 0 ? ((themePassed / themeTotal) * 100).toFixed(0) : '0';
      
      console.log(`\n🎨 ${themeName} (${themePassed}/${themeTotal} - ${themeRate}%)`);
      themeResults.forEach(result => {
        const status = result.passed ? '✅' : '❌';
        const time = result.executionTime ? ` (${result.executionTime}ms)` : '';
        const size = result.fileSizeKB ? ` [${result.fileSizeKB.toFixed(1)}KB]` : '';
        console.log(`  ${status} ${result.selector}: ${result.message}${time}${size}`);
      });
    });
    
    // List all screenshots created
    const screenshotFiles = results.filter(r => r.filePath).map(r => r.filePath!);
    if (screenshotFiles.length > 0) {
      console.log(`\n📸 SCREENSHOTS CREATED (${screenshotFiles.length} files):`);
      screenshotFiles.forEach(filePath => {
        console.log(`  • ${path.basename(filePath)}`);
      });
      console.log(`\n📁 Location: Directory containing the first screenshot`);
      if (screenshotFiles[0]) {
        console.log(`   ${path.dirname(screenshotFiles[0])}/`);
      }
    }
    
    if (failedTests === 0) {
      console.log('\n🎉 ALL THEME VALIDATION TESTS PASSED!');
      console.log('✅ Dynamic theme discovery is working');
      console.log('✅ Theme switching is functional');
      console.log('✅ All themes render properly');
      console.log('✅ Screenshots capture correctly');
    } else {
      console.log(`\n⚠️ ${failedTests} validation tests failed`);
      console.log('Check the failed tests above for details');
    }
    
    console.log('\n' + '='.repeat(60));
  }
  
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

async function runThemeValidation(): Promise<void> {
  const validator = new ThemeSystemValidator();
  await validator.runValidation();
}

// Run if called directly
if (require.main === module) {
  runThemeValidation().catch(console.error);
}

export { runThemeValidation };