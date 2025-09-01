/**
 * ThemeSet Browser Command - Execute theme switching in browser environment
 */

import type { ThemeSetParams, ThemeSetResult } from '../shared/ThemeSetTypes';
import { createThemeSetResult } from '../shared/ThemeSetTypes';
import { EnhancementError } from '../../../../system/core/types/ErrorTypes';

export class ThemeSetBrowserCommand {
  async execute(params: ThemeSetParams): Promise<ThemeSetResult> {
    // Handle CLI positional arguments - if themeName is undefined, use first positional argument
    let themeName = params.themeName;
    if (!themeName && (params as any)._positional && (params as any)._positional.length > 0) {
      themeName = (params as any)._positional[0];
      console.log(`🔧 ThemeSetBrowser: Using positional argument '${themeName}' as theme name`);
    }
    
    try {
      
      console.log(`🎨 ThemeSetBrowser: Setting theme to '${themeName}'`);
      
      // Get current theme before switching
      const previousTheme = await this.getCurrentTheme();
      
      // Try multiple methods to set the theme
      let success = false;
      let method = 'none';
      
      // Method 1: Use ThemeWidget.setTheme if available
      const themeWidget = document.querySelector('theme-widget') as any;
      if (themeWidget && typeof themeWidget.setTheme === 'function') {
        try {
          await themeWidget.setTheme(themeName);
          success = true;
          method = 'ThemeWidget.setTheme';
          console.log(`✅ ThemeSetBrowser: Theme set using ThemeWidget.setTheme`);
        } catch (error) {
          console.warn(`⚠️ ThemeSetBrowser: ThemeWidget.setTheme failed:`, error);
        }
      }
      
      // Method 2: Use theme selector dropdown
      if (!success) {
        const themeSelector = document.querySelector('#theme-selector') as HTMLSelectElement || 
                             document.querySelector('theme-widget select') as HTMLSelectElement;
        
        if (themeSelector) {
          try {
            // Check if the theme option exists
            const option = Array.from(themeSelector.options).find(opt => opt.value === themeName);
            
            if (option) {
              themeSelector.value = themeName;
              
              // Trigger change event
              const changeEvent = new Event('change', { bubbles: true });
              themeSelector.dispatchEvent(changeEvent);
              
              // Click apply button if available
              const applyButton = document.querySelector('#apply-theme') as HTMLButtonElement;
              if (applyButton) {
                applyButton.click();
              }
              
              success = true;
              method = 'dropdown selection';
              console.log(`✅ ThemeSetBrowser: Theme set using dropdown selection`);
            } else {
              console.warn(`⚠️ ThemeSetBrowser: Theme option '${themeName}' not found in dropdown`);
            }
          } catch (error) {
            console.warn(`⚠️ ThemeSetBrowser: Dropdown method failed:`, error);
          }
        }
      }
      
      // Method 3: Direct CSS manipulation (fallback)
      if (!success) {
        try {
          // Check if there are existing theme style elements we can manipulate
          const existingThemeStyle = document.head.querySelector('[id^="jtag-theme-"]') as HTMLStyleElement;
          
          if (existingThemeStyle) {
            // For now, just log that we found theme infrastructure
            console.log(`🔍 ThemeSetBrowser: Found existing theme infrastructure: ${existingThemeStyle.id}`);
            // This would require loading the actual theme CSS, which should be done via ThemeWidget
            success = false; // Keep as false since we're not actually applying the theme
            method = 'css manipulation detected';
          }
        } catch (error) {
          console.warn(`⚠️ ThemeSetBrowser: CSS manipulation method failed:`, error);
        }
      }
      
      if (success) {
        return createThemeSetResult(params.context, params.sessionId, {
          success: true,
          themeName: themeName,
          previousTheme,
          applied: true
        });
      } else {
        const error = new EnhancementError(
          'theme-set',
          `Failed to set theme '${themeName}': No available method succeeded`
        );
        
        return createThemeSetResult(params.context, params.sessionId, {
          success: false,
          themeName: themeName,
          previousTheme,
          applied: false,
          error
        });
      }
      
    } catch (error) {
      const jtagError = new EnhancementError(
        'theme-set',
        `Unexpected error setting theme '${themeName}': ${error}`
      );
      
      return createThemeSetResult(params.context, params.sessionId, {
        success: false,
        themeName: themeName,
        applied: false,
        error: jtagError
      });
    }
  }
  
  private async getCurrentTheme(): Promise<string | undefined> {
    try {
      // Try to get current theme from ThemeWidget
      const themeWidget = document.querySelector('theme-widget') as any;
      if (themeWidget && typeof themeWidget.getCurrentTheme === 'function') {
        return themeWidget.getCurrentTheme();
      }
      
      // Try to get from theme selector
      const themeSelector = document.querySelector('#theme-selector') as HTMLSelectElement ||
                           document.querySelector('theme-widget select') as HTMLSelectElement;
      
      if (themeSelector && themeSelector.value) {
        return themeSelector.value;
      }
      
      // Try to get from theme style element ID
      const themeStyle = document.head.querySelector('[id^="jtag-theme-"]') as HTMLStyleElement;
      if (themeStyle && themeStyle.id) {
        const match = themeStyle.id.match(/^jtag-theme-(.+)$/);
        if (match) {
          return match[1];
        }
      }
      
      return undefined;
    } catch (error) {
      console.warn('⚠️ ThemeSetBrowser: Could not determine current theme:', error);
      return undefined;
    }
  }
}