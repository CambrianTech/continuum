/**
 * Theme Screenshot Integration Test
 * 
 * Takes a screenshot of each theme for visual regression testing
 * Uses persistent browser session to maintain state across commands
 */

import { JTAGClientBrowser } from '../../system/core/client/browser/JTAGClientBrowser';

async function takeThemeScreenshots() {
  console.log('🎨 Theme Screenshot Integration Test');
  console.log('===================================');
  console.log('📝 Using persistent browser session for consistent state');
  
  let jtagSystem: any = null;
  
  try {
    // Connect to JTAG system with persistent session
    console.log('🔌 Connecting to JTAG system...');
    const { client } = await JTAGClientBrowser.connectLocal();
    jtagSystem = client;
    
    console.log('✅ Connected to JTAG system');
    
    // Get list of available themes
    console.log('📋 Getting theme list...');
    const themeListResult = await jtagSystem.commands['theme/list']({});
    
    if (!themeListResult.success) {
      throw new Error(`Failed to get theme list: ${themeListResult.error}`);
    }
    
    const themes = themeListResult.themes || [];
    console.log(`🎨 Found ${themes.length} themes: ${themes.join(', ')}`);
    
    const results: Array<{ theme: string; success: boolean; filepath?: string; error?: string }> = [];
    
    // Take screenshot of each theme using the same persistent session
    for (const themeName of themes) {
      console.log(`\n📸 Capturing theme: ${themeName}`);
      
      try {
        // Switch to the theme using theme command
        console.log(`🎨 Switching to theme ${themeName}...`);
        const switchResult = await jtagSystem.commands['theme/set']({ themeName });
        
        if (switchResult.success) {
          console.log(`✅ Theme switched to: ${themeName}`);
        } else {
          console.log(`⚠️ Theme switch may have failed: ${switchResult.error || 'Unknown error'}`);
        }
        
        // Wait for visual changes to apply
        await new Promise(resolve => setTimeout(resolve, 1500));
        
        // Take screenshot
        const screenshotResult = await jtagSystem.commands.screenshot({
          querySelector: 'body',
          filename: `theme-${themeName}.png`
        });
        
        if (screenshotResult.success && screenshotResult.filepath) {
          console.log(`✅ Screenshot saved: theme-${themeName}.png`);
          console.log(`   Path: ${screenshotResult.filepath}`);
          results.push({
            theme: themeName,
            success: true,
            filepath: screenshotResult.filepath
          });
        } else {
          console.log(`❌ Screenshot failed for ${themeName}: ${screenshotResult.error || 'Unknown error'}`);
          results.push({
            theme: themeName,
            success: false,
            error: screenshotResult.error || 'Screenshot command failed'
          });
        }
        
      } catch (error) {
        console.log(`❌ Error with theme ${themeName}: ${error}`);
        results.push({
          theme: themeName,
          success: false,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }
    
    // Print summary
    console.log('\n🎨 THEME SCREENSHOT SUMMARY');
    console.log('============================');
    
    const successful = results.filter(r => r.success).length;
    const total = results.length;
    
    console.log(`📊 Success Rate: ${successful}/${total} (${Math.round(successful/total*100)}%)`);
    console.log(`📁 All screenshots saved to same session directory for easy comparison!`);
    
    for (const result of results) {
      const status = result.success ? '✅' : '❌';
      console.log(`${status} ${result.theme}: ${result.success ? result.filepath : result.error}`);
    }
    
    if (successful === 0) {
      throw new Error('No themes were successfully captured');
    }
    
    return { success: successful > 0, results };
    
  } catch (error) {
    console.error(`💥 Test error: ${error}`);
    throw error;
  }
}

// Run the test
if (require.main === module) {
  takeThemeScreenshots().then(result => {
    console.log('\n✅ Theme screenshot test completed successfully!');
    process.exit(0);
  }).catch(error => {
    console.error('\n❌ Theme screenshot test failed!');
    process.exit(1);
  });
}

export { takeThemeScreenshots };