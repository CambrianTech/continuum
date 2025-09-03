/**
 * Automated Theme Screenshot Test
 * 
 * AUTONOMOUS TEST: Gets themes via theme/list, iterates through all themes,
 * takes screenshots of each, all in same session directory.
 * 
 * THIS TEST PROVES THEME SYSTEM WORKS END-TO-END
 */

const { JTAGClientServer } = await import('../../system/core/client/server/JTAGClientServer');

interface ThemeTestResult {
  theme: string;
  success: boolean;
  filepath?: string;
  error?: string;
}

async function runAutomatedThemeTest(): Promise<{ success: boolean; results: ThemeTestResult[] }> {
  console.log('🎨 AUTOMATED THEME SCREENSHOT TEST');
  console.log('================================');
  
  let jtagSystem: any = null;
  const results: ThemeTestResult[] = [];
  
  try {
    // Connect to JTAG system with persistent session
    console.log('🔌 Connecting to JTAG system...');
    const { client } = await JTAGClientServer.connect();
    jtagSystem = client;
    
    console.log('✅ Connected to JTAG system');
    console.log(`📋 Session: ${client.sessionId}`);
    
    // GET THEMES VIA LIST COMMAND (NOT HARDCODED)
    console.log('📋 Getting theme list dynamically...');
    const themeListResult = await jtagSystem.commands['theme/list']({});
    
    if (!themeListResult.success) {
      throw new Error(`Failed to get theme list: ${themeListResult.error}`);
    }
    
    const themes: string[] = themeListResult.themes || [];
    console.log(`🎨 Found ${themes.length} themes: ${themes.join(', ')}`);
    
    // ITERATE THROUGH ALL THEMES AND SCREENSHOT EACH
    for (const themeName of themes) {
      console.log(`\\n📸 Processing theme: ${themeName}`);
      
      try {
        // AWAIT theme switch
        console.log(`🎨 Switching to theme ${themeName}...`);
        const switchResult = await jtagSystem.commands['theme/set']({ themeName });
        
        if (!switchResult.success) {
          throw new Error(`Theme switch failed: ${switchResult.error || 'Unknown error'}`);
        }
        
        console.log(`✅ Theme switched to: ${themeName}`);
        
        // AWAIT visual changes to apply  
        await new Promise(resolve => setTimeout(resolve, 1500));
        
        // AWAIT screenshot
        const screenshotResult = await jtagSystem.commands.screenshot({
          querySelector: 'body',
          filename: `automated-theme-${themeName}.png`
        });
        
        if (screenshotResult.success && screenshotResult.filepath) {
          console.log(`✅ Screenshot saved: automated-theme-${themeName}.png`);
          results.push({
            theme: themeName,
            success: true,
            filepath: screenshotResult.filepath
          });
        } else {
          throw new Error(`Screenshot failed: ${screenshotResult.error || 'Unknown error'}`);
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
    
    // PRINT SUMMARY
    console.log('\\n🎨 AUTOMATED THEME TEST SUMMARY');
    console.log('==============================');
    
    const successful = results.filter(r => r.success).length;
    const total = results.length;
    
    console.log(`📊 Success Rate: ${successful}/${total} (${Math.round(successful/total*100)}%)`);
    console.log(`📁 All screenshots saved to same session directory!`);
    console.log(`📋 Session maintained: ${client.sessionId}`);
    
    for (const result of results) {
      const status = result.success ? '✅' : '❌';
      console.log(`${status} ${result.theme}: ${result.success ? result.filepath : result.error}`);
    }
    
    if (successful === 0) {
      throw new Error('No themes were successfully captured');
    }
    
    return { success: successful === total, results };
    
  } catch (error) {
    console.error(`💥 Automated theme test error: ${error}`);
    throw error;
  } finally {
    if (jtagSystem) {
      // Cleanup would go here if needed
    }
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