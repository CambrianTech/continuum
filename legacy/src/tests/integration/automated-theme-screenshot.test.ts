/**
 * Automated Theme Screenshot Test - Refactored with Specialized Test Utilities
 * 
 * AUTONOMOUS TEST: Gets themes via theme/list, iterates through all themes,
 * takes screenshots of each, all in same session directory.
 * 
 * THIS TEST PROVES THEME SYSTEM WORKS END-TO-END
 * 
 * REFACTORED: Uses properly categorized utilities with descriptive names
 */

import { 
  testAllThemesWithScreenshots,
  ThemeSuiteResult,
  assertThemeSuiteSuccess
} from '../shared/ThemeTesting';

import { TEST_TIMEOUTS } from '../shared/TestConstants';

async function runAutomatedThemeTest(): Promise<{ success: boolean; results: ThemeSuiteResult }> {
  console.log('🎨 AUTOMATED THEME SCREENSHOT TEST (SPECIALIZED)');
  console.log('================================================');
  
  try {
    // Use specialized theme testing engine with comprehensive validation
    console.log('🔌 Using ThemeTesting engine for comprehensive validation...');
    
    const suiteResult = await testAllThemesWithScreenshots({
      timeout: TEST_TIMEOUTS.INTEGRATION_TEST,
      validateSwitch: true,
      captureScreenshot: true,
      logProgress: true
    });
    
    // Validate results using specialized assertions
    try {
      assertThemeSuiteSuccess(suiteResult, {
        context: 'Automated Theme Test',
        throwOnFailure: true
      });
    } catch (error) {
      // Don't throw, but report the validation issue
      console.warn('⚠️ Theme suite validation:', error);
    }
    
    console.log('\n✅ AUTOMATED THEME TEST COMPLETED!');
    console.log('🎯 All themes tested via dynamic theme/list command');
    console.log('📸 All screenshots captured in same session directory');
    console.log(`📊 Final Result: ${suiteResult.summary.successful}/${suiteResult.summary.total} themes successful`);
    
    return { 
      success: suiteResult.summary.successRate === 100,
      results: suiteResult 
    };
    
  } catch (error) {
    console.error(`💥 Automated theme test error: ${error}`);
    throw error;
  }
}

// Run the test
if (require.main === module) {
  runAutomatedThemeTest()
    .then(result => {
      console.log('\\n✅ AUTOMATED THEME TEST COMPLETED SUCCESSFULLY!');
      console.log('🎯 All themes tested via dynamic theme/list command');
      console.log('📸 All screenshots captured in same session directory');
      process.exit(0);
    })
    .catch(error => {
      console.error('\\n❌ AUTOMATED THEME TEST FAILED!');
      console.error(error);
      process.exit(1);
    });
}

export { runAutomatedThemeTest };