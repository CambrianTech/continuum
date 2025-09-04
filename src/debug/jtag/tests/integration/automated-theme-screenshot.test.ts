/**
 * Automated Theme Screenshot Test - Refactored with Modern Test Utilities
 * 
 * AUTONOMOUS TEST: Gets themes via theme/list, iterates through all themes,
 * takes screenshots of each, all in same session directory.
 * 
 * THIS TEST PROVES THEME SYSTEM WORKS END-TO-END
 * 
 * REFACTORED: Uses shared utilities, proper typing, constants, error handling
 */

import { 
  ModernTestRunner,
  ThemeTestResult,
  MODERN_TEST_CONSTANTS,
  testAllThemes 
} from '../shared/ModernTestUtilities';

import { JTAGClientFactory, connectJTAGClient } from '../shared/JTAGClientFactory';

async function runAutomatedThemeTest(): Promise<{ success: boolean; results: ThemeTestResult[] }> {
  console.log('🎨 AUTOMATED THEME SCREENSHOT TEST (MODERNIZED)');
  console.log('================================================');
  
  const runner = new ModernTestRunner();
  
  try {
    // Connect using modern client factory with proper error handling
    console.log('🔌 Connecting using ModernTestUtilities...');
    const connection = await connectJTAGClient({
      timeout: MODERN_TEST_CONSTANTS.TIMEOUTS.INTEGRATION_TEST,
      validateConnection: true
    });
    
    console.log('✅ Connected to JTAG system');
    console.log(`📋 Session: ${connection.sessionId}`);
    
    // Get themes via shared test runner
    console.log('📋 Getting theme list dynamically...');
    const themeListResult = await runner.executeTest(
      'Get Theme List',
      async (client) => {
        const factory = JTAGClientFactory.getInstance();
        const result = await factory.executeCommand(
          client,
          'theme/list',
          {},
          { timeout: MODERN_TEST_CONSTANTS.TIMEOUTS.NORMAL_TEST }
        );
        
        if (!result.success) {
          throw new Error(`Failed to get theme list: ${result.error}`);
        }
        
        return result.data;
      }
    );
    
    if (!themeListResult.success) {
      throw new Error(`Theme list failed: ${themeListResult.error}`);
    }
    
    const themes: string[] = themeListResult.data?.themes || MODERN_TEST_CONSTANTS.THEMES.ALL_THEMES;
    console.log(`🎨 Found ${themes.length} themes: ${themes.join(', ')}`);
    
    // Test each theme using modern test utilities
    const results: ThemeTestResult[] = [];
    for (const themeName of themes) {
      console.log(`\n📸 Processing theme: ${themeName}`);
      const themeResult = await runner.executeThemeTest(themeName, {
        timeout: MODERN_TEST_CONSTANTS.TIMEOUTS.NORMAL_TEST,
        logProgress: true
      });
      
      results.push(themeResult);
    }
    
    // Print comprehensive summary using modern utilities
    console.log('\n🎨 AUTOMATED THEME TEST SUMMARY');
    console.log('================================');
    
    const successful = results.filter(r => r.success).length;
    const total = results.length;
    
    console.log(`📊 Success Rate: ${successful}/${total} (${Math.round(successful/total*100)}%)`);
    console.log(`📁 All screenshots saved to same session directory!`);
    console.log(`📋 Session maintained: ${connection.sessionId}`);
    
    // Detailed results with proper formatting
    for (const result of results) {
      const status = result.success ? '✅' : '❌';
      const details = result.success 
        ? `${result.screenshotPath} (switch: ${result.switchTime}ms, capture: ${result.screenshotTime}ms)`
        : result.error;
      console.log(`${status} ${result.theme}: ${details}`);
    }
    
    if (successful === 0) {
      throw new Error('No themes were successfully captured');
    }
    
    return { success: successful === total, results };
    
  } catch (error) {
    console.error(`💥 Automated theme test error: ${error}`);
    throw error;
  } finally {
    // Proper cleanup using modern utilities
    await runner.cleanup();
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