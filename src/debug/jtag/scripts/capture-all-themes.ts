#!/usr/bin/env tsx
/**
 * Theme Screenshot Capture Script
 * Automatically captures screenshots of all available themes for documentation and testing
 */

import { JTAGClient } from '../system/jtag-client/shared/JTAGClient';
import { JTAGClientServer } from '../system/jtag-client/server/JTAGClientServer';

interface ThemeInfo {
  name: string;
  displayName: string;
  description: string;
  category: string;
}

async function main() {
  console.log('🎨 Theme Screenshot Capture Script Starting...');
  
  let jtagClient: JTAGClient | null = null;
  
  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    jtagClient = new JTAGClientServer();
    await jtagClient.connect();
    console.log('✅ Connected to JTAG system');
    
    // Get list of all available themes dynamically
    console.log('🔍 Discovering available themes...');
    const availableThemes = await getAvailableThemes(jtagClient);
    console.log(`📋 Found ${availableThemes.length} themes:`, availableThemes.map(t => t.name));
    
    // Take initial full page screenshot for comparison
    console.log('📸 Taking initial full page screenshot...');
    await jtagClient.commands.screenshot({
      querySelector: 'body',
      filename: 'theme-comparison-initial.png'
    });
    
    // Iterate through each theme and capture screenshots
    for (const theme of availableThemes) {
      console.log(`\n🎨 Capturing theme: ${theme.name} (${theme.displayName})`);
      
      try {
        // Switch to this theme
        console.log(`🔄 Switching to theme '${theme.name}'...`);
        await switchToTheme(jtagClient, theme.name);
        
        // Wait a moment for theme to apply
        await sleep(1000);
        
        // Take full page screenshot
        const filename = `theme-${theme.name}-full-page.png`;
        console.log(`📸 Capturing full page screenshot: ${filename}`);
        await jtagClient.commands.screenshot({
          querySelector: 'body',
          filename: filename
        });
        
        // Take sidebar-specific screenshot
        const sidebarFilename = `theme-${theme.name}-sidebar.png`;
        console.log(`📸 Capturing sidebar screenshot: ${sidebarFilename}`);
        try {
          await jtagClient.commands.screenshot({
            querySelector: 'continuum-sidebar',
            filename: sidebarFilename
          });
        } catch (error) {
          console.warn(`⚠️ Could not capture sidebar for ${theme.name}:`, (error as Error).message);
        }
        
        // Take chat widget screenshot
        const chatFilename = `theme-${theme.name}-chat.png`;
        console.log(`📸 Capturing chat widget screenshot: ${chatFilename}`);
        try {
          await jtagClient.commands.screenshot({
            querySelector: 'chat-widget',
            filename: chatFilename
          });
        } catch (error) {
          console.warn(`⚠️ Could not capture chat widget for ${theme.name}:`, (error as Error).message);
        }
        
        console.log(`✅ Theme '${theme.name}' captured successfully`);
        
      } catch (error) {
        console.error(`❌ Failed to capture theme '${theme.name}':`, error);
      }
    }
    
    // Generate summary report
    console.log('\n📊 Generating theme capture summary...');
    const summaryReport = generateSummaryReport(availableThemes);
    console.log('\n' + summaryReport);
    
    console.log('\n🎉 Theme screenshot capture complete!');
    console.log('📁 Screenshots saved to: .continuum/jtag/sessions/user/[SESSION_ID]/screenshots/');
    
  } catch (error) {
    console.error('❌ Theme capture script failed:', error);
    process.exit(1);
  } finally {
    if (jtagClient) {
      console.log('🔌 Disconnecting from JTAG system...');
      await jtagClient.disconnect();
    }
  }
}

/**
 * Get list of available themes from the theme system
 */
async function getAvailableThemes(jtagClient: JTAGClient): Promise<ThemeInfo[]> {
  try {
    // Execute code in browser to get theme registry information
    const result = await jtagClient.commands.exec({
      code: `
        // Try to get themes from ThemeRegistry if available
        if (window.ThemeRegistry && window.ThemeRegistry.getAllThemes) {
          return window.ThemeRegistry.getAllThemes().map(theme => ({
            name: theme.name,
            displayName: theme.displayName,
            description: theme.description,
            category: theme.category
          }));
        } else {
          // Fallback to checking theme dropdown if ThemeRegistry not available
          const themeSelector = document.querySelector('#theme-selector') || document.querySelector('theme-widget select');
          if (themeSelector) {
            return Array.from(themeSelector.options).map(option => ({
              name: option.value,
              displayName: option.textContent || option.value,
              description: option.title || 'Theme',
              category: 'unknown'
            }));
          }
          
          // Fallback to known themes if nothing else works
          return [
            { name: 'base', displayName: 'Base - Dark Cyberpunk', description: 'Default cyberpunk theme', category: 'dark' },
            { name: 'light', displayName: 'Light - Clean Professional', description: 'Clean light theme', category: 'light' },
            { name: 'cyberpunk', displayName: 'Cyberpunk - Neon Future', description: 'Bright neon cyberpunk theme', category: 'dark' },
            { name: 'retro-mac', displayName: 'Retro Mac - System 11', description: 'Classic Mac OS aesthetics', category: 'retro' },
            { name: 'monochrome', displayName: 'Monochrome - High Contrast', description: 'High contrast accessibility theme', category: 'accessibility' },
            { name: 'classic', displayName: 'Classic - Professional', description: 'Traditional professional interface', category: 'professional' }
          ];
        }
      `,
      environment: 'browser'
    });
    
    if (result.success && result.result) {
      return result.result as ThemeInfo[];
    } else {
      throw new Error('Failed to get theme information from browser');
    }
  } catch (error) {
    console.warn('⚠️ Could not get themes dynamically, using fallback list');
    // Fallback to known themes
    return [
      { name: 'base', displayName: 'Base - Dark Cyberpunk', description: 'Default cyberpunk theme', category: 'dark' },
      { name: 'light', displayName: 'Light - Clean Professional', description: 'Clean light theme', category: 'light' },
      { name: 'cyberpunk', displayName: 'Cyberpunk - Neon Future', description: 'Bright neon cyberpunk theme', category: 'dark' },
      { name: 'retro-mac', displayName: 'Retro Mac - System 11', description: 'Classic Mac OS aesthetics', category: 'retro' },
      { name: 'monochrome', displayName: 'Monochrome - High Contrast', description: 'High contrast accessibility theme', category: 'accessibility' },
      { name: 'classic', displayName: 'Classic - Professional', description: 'Traditional professional interface', category: 'professional' }
    ];
  }
}

/**
 * Switch to a specific theme
 */
async function switchToTheme(jtagClient: JTAGClient, themeName: string): Promise<void> {
  try {
    // Execute theme switch in browser
    const result = await jtagClient.commands.exec({
      code: `
        // Try multiple methods to switch themes
        let success = false;
        
        // Method 1: Use ThemeWidget if available
        const themeWidget = document.querySelector('theme-widget');
        if (themeWidget && themeWidget.setTheme) {
          await themeWidget.setTheme('${themeName}');
          success = true;
          return { success: true, method: 'ThemeWidget.setTheme' };
        }
        
        // Method 2: Use theme selector dropdown
        const themeSelector = document.querySelector('#theme-selector') || document.querySelector('theme-widget select');
        if (themeSelector) {
          themeSelector.value = '${themeName}';
          // Trigger change event
          const changeEvent = new Event('change', { bubbles: true });
          themeSelector.dispatchEvent(changeEvent);
          success = true;
          return { success: true, method: 'dropdown change event' };
        }
        
        // Method 3: Try to find and click theme option
        const themeOption = Array.from(document.querySelectorAll('option')).find(opt => opt.value === '${themeName}');
        if (themeOption) {
          themeOption.selected = true;
          const selectElement = themeOption.parentElement;
          if (selectElement) {
            const changeEvent = new Event('change', { bubbles: true });
            selectElement.dispatchEvent(changeEvent);
            success = true;
            return { success: true, method: 'option selection' };
          }
        }
        
        return { success: false, error: 'No theme switching method found' };
      `,
      environment: 'browser'
    });
    
    if (!result.success || !result.result?.success) {
      console.warn(`⚠️ Theme switch may not have worked for '${themeName}'`);
    } else {
      console.log(`✅ Theme switched using method: ${result.result.method}`);
    }
  } catch (error) {
    console.warn(`⚠️ Failed to switch to theme '${themeName}':`, error);
  }
}

/**
 * Sleep utility function
 */
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Generate summary report of captured themes
 */
function generateSummaryReport(themes: ThemeInfo[]): string {
  const categorizedThemes = themes.reduce((acc, theme) => {
    if (!acc[theme.category]) {
      acc[theme.category] = [];
    }
    acc[theme.category].push(theme);
    return acc;
  }, {} as Record<string, ThemeInfo[]>);
  
  let report = '📊 THEME CAPTURE SUMMARY\n';
  report += '=' + '='.repeat(25) + '\n\n';
  report += `Total themes captured: ${themes.length}\n\n`;
  
  Object.entries(categorizedThemes).forEach(([category, categoryThemes]) => {
    report += `📂 ${category.toUpperCase()}\n`;
    categoryThemes.forEach(theme => {
      report += `  • ${theme.displayName} (${theme.name})\n`;
      report += `    ${theme.description}\n`;
    });
    report += '\n';
  });
  
  report += '📁 Screenshot Files Generated:\n';
  themes.forEach(theme => {
    report += `  • theme-${theme.name}-full-page.png\n`;
    report += `  • theme-${theme.name}-sidebar.png\n`;
    report += `  • theme-${theme.name}-chat.png\n`;
  });
  
  return report;
}

// Run the script
if (require.main === module) {
  main().catch(console.error);
}

export { main as captureAllThemes };