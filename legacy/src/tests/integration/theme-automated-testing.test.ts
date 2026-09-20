/**
 * Automated Theme Testing - Complete UI Theme Validation
 * 
 * This test replicates the test-all-themes.sh functionality in TypeScript.
 * Gets available themes dynamically and screenshots each one for visual validation.
 * 
 * Validates:
 * - Dynamic theme discovery (not hardcoded)
 * - Theme switching functionality 
 * - Visual changes with screenshots
 * - Complete theme coverage
 * - Theme system reliability
 */

import { JTAGClientFactory } from '../../api/shared/JTAGClientFactory';
import type { JTAGClient } from '../../api/shared/types/JTAGClientTypes';
import type { UUID } from '../../system/core/types/UUID';

interface ThemeListResult {
  themes: string[];
}

interface ThemeTestResult {
  totalThemes: number;
  successfulTests: number;
  failedThemes: string[];
  screenshotsTaken: number;
}

async function testAllThemesAutomated() {
  console.log('🎨 AUTOMATED THEME TESTING');
  console.log('=========================');

  let client: JTAGClient | null = null;

  try {
    // Connect to JTAG system
    client = await JTAGClientFactory.createClient({
      sessionId: 'deadbeef-cafe-4bad-8ace-5e551000c0de' as UUID,
      environment: 'server'
    });

    console.log('✅ Connected to JTAG system');

    // Get theme list dynamically
    console.log('📋 Getting theme list...');
    const themeListResult = await client.commands.theme.list();
    console.log('Theme list result:', themeListResult.data);

    const themes = (themeListResult.data as ThemeListResult).themes;

    if (!themes || themes.length === 0) {
      throw new Error('No themes found in system');
    }

    console.log('');
    console.log('🎨 Found themes:');
    themes.forEach(theme => {
      console.log(`  - ${theme}`);
    });

    console.log('');
    console.log('📸 Starting theme screenshot test...');

    const results: ThemeTestResult = {
      totalThemes: themes.length,
      successfulTests: 0,
      failedThemes: [],
      screenshotsTaken: 0
    };

    // Iterate through each theme
    for (const theme of themes) {
      console.log('');
      console.log(`🔄 Testing theme: ${theme}`);
      console.log('-------------------------');

      try {
        // Set theme
        console.log(`🎨 Setting theme to ${theme}...`);
        const setResult = await client.commands.theme.set(theme);
        
        if (!setResult.success) {
          console.log(`❌ Failed to set theme ${theme}:`, setResult.error);
          results.failedThemes.push(theme);
          continue;
        }

        console.log(`✅ Theme set to ${theme}`);

        // Give time for visual changes
        await new Promise(resolve => setTimeout(resolve, 2000));

        // Take screenshot
        console.log('📸 Taking screenshot...');
        const screenshotResult = await client.commands.screenshot({
          querySelector: 'body',
          filename: `automated-theme-${theme}.png`
        });

        if (screenshotResult.success) {
          console.log(`✅ Screenshot saved: automated-theme-${theme}.png`);
          results.screenshotsTaken++;
        } else {
          console.log(`⚠️ Screenshot failed for theme ${theme}:`, screenshotResult.error);
        }

        console.log(`✅ Completed: ${theme}`);
        results.successfulTests++;

      } catch (error) {
        console.log(`❌ Error testing theme ${theme}:`, error.message);
        results.failedThemes.push(theme);
      }
    }

    console.log('');
    console.log('🎉 THEME TEST COMPLETE!');
    console.log('======================');
    console.log(`📊 Results: ${results.successfulTests}/${results.totalThemes} themes tested successfully`);
    console.log(`📸 Screenshots taken: ${results.screenshotsTaken}`);
    
    if (results.failedThemes.length > 0) {
      console.log(`❌ Failed themes: ${results.failedThemes.join(', ')}`);
    }

    console.log('📁 All screenshots saved to session directory');
    console.log('🔍 Check .continuum/sessions/user/*/screenshots/ for results');

    // Test assertions
    console.log('');
    console.log('🧪 TEST ASSERTIONS:');
    console.log('==================');

    const assertions = [
      {
        test: 'Themes discovered',
        passed: results.totalThemes > 0,
        actual: results.totalThemes
      },
      {
        test: 'At least 5 themes available',
        passed: results.totalThemes >= 5,
        actual: results.totalThemes
      },
      {
        test: 'All theme switches successful',
        passed: results.successfulTests === results.totalThemes,
        actual: `${results.successfulTests}/${results.totalThemes}`
      },
      {
        test: 'Screenshots taken for all themes',
        passed: results.screenshotsTaken === results.totalThemes,
        actual: results.screenshotsTaken
      },
      {
        test: 'No failed themes',
        passed: results.failedThemes.length === 0,
        actual: results.failedThemes.length
      }
    ];

    let allPassed = true;
    assertions.forEach(({ test, passed, actual }) => {
      const status = passed ? '✅ PASS' : '❌ FAIL';
      console.log(`${status}: ${test} (${actual})`);
      if (!passed) allPassed = false;
    });

    console.log('');
    console.log('🎨 THEMES TESTED:');
    themes.forEach((theme, index) => {
      const status = results.failedThemes.includes(theme) ? '❌' : '✅';
      console.log(`   ${index + 1}. ${status} ${theme}`);
    });

    if (allPassed) {
      console.log('');
      console.log('🎉 AUTOMATED THEME TEST: ALL ASSERTIONS PASSED!');
      console.log('🎨 All themes are working correctly and can be switched');
      return { success: true, results };
    } else {
      console.log('');
      console.log('💥 AUTOMATED THEME TEST: SOME ASSERTIONS FAILED!');
      console.log('🔧 Check the detailed analysis above for issues');
      return { success: false, results };
    }

  } catch (error) {
    console.error('❌ Automated theme test failed with error:', error);
    return { success: false, error: error.message };
  } finally {
    if (client) {
      await client.disconnect();
    }
  }
}

// Run the test if this file is executed directly
if (require.main === module) {
  testAllThemesAutomated().then(result => {
    console.log('');
    console.log('🏁 Test completed:', result.success ? 'SUCCESS' : 'FAILURE');
    process.exit(result.success ? 0 : 1);
  }).catch(error => {
    console.error('💥 Test execution failed:', error);
    process.exit(1);
  });
}

export { testAllThemesAutomated };
export type { ThemeTestResult };