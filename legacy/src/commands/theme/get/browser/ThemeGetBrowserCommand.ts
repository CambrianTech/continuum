/**
 * ThemeGet Browser Command - Get current theme information in browser environment
 */

import type { ThemeGetParams, ThemeGetResult } from '../shared/ThemeGetTypes';
import type { ThemeManifest } from '../../shared/ThemeTypes';
import { createThemeGetResult } from '../shared/ThemeGetTypes';
import { EnhancementError } from '../../../../system/core/types/ErrorTypes';

export class ThemeGetBrowserCommand {
  async execute(params: ThemeGetParams): Promise<ThemeGetResult> {
    try {
      console.log(`🎨 ThemeGetBrowser: Getting current theme information`);
      
      // Get current theme name
      const currentTheme = await this.getCurrentTheme();
      
      if (!currentTheme) {
        const error = new EnhancementError(
          'theme-get',
          'Could not determine current theme'
        );
        
        return createThemeGetResult(params.context, params.sessionId, {
          success: false,
          currentTheme: 'unknown',
          themeApplied: false,
          error
        });
      }
      
      // Try to get theme manifest
      const themeManifest = await this.getThemeManifest(currentTheme);
      
      // Check if theme is actually applied
      const themeApplied = await this.isThemeApplied(currentTheme);
      
      console.log(`✅ ThemeGetBrowser: Current theme is '${currentTheme}' (applied: ${themeApplied})`);
      
      return createThemeGetResult(params.context, params.sessionId, {
        success: true,
        currentTheme,
        themeManifest,
        themeApplied
      });
      
    } catch (error) {
      const jtagError = new EnhancementError(
        'theme-get',
        `Unexpected error getting current theme: ${error}`
      );
      
      return createThemeGetResult(params.context, params.sessionId, {
        success: false,
        currentTheme: 'error',
        themeApplied: false,
        error: jtagError
      });
    }
  }
  
  private async getCurrentTheme(): Promise<string | null> {
    try {
      // Method 1: Get from UniverseWidget
      const themeWidget = document.querySelector('universe-widget') as any;
      if (themeWidget && typeof themeWidget.getCurrentTheme === 'function') {
        const theme = themeWidget.getCurrentTheme();
        if (theme) {
          console.log(`🔍 ThemeGetBrowser: Got theme from UniverseWidget: '${theme}'`);
          return theme;
        }
      }
      
      // Method 2: Get from theme selector dropdown
      const themeSelector = document.querySelector('#theme-selector') as HTMLSelectElement ||
                           document.querySelector('universe-widget select') as HTMLSelectElement;
      
      if (themeSelector && themeSelector.value) {
        console.log(`🔍 ThemeGetBrowser: Got theme from dropdown: '${themeSelector.value}'`);
        return themeSelector.value;
      }
      
      // Method 3: Parse from theme style element ID
      const themeStyle = document.head.querySelector('[id^="jtag-theme-"]') as HTMLStyleElement;
      if (themeStyle && themeStyle.id) {
        const match = themeStyle.id.match(/^jtag-theme-(.+)$/);
        if (match && match[1]) {
          console.log(`🔍 ThemeGetBrowser: Got theme from style element: '${match[1]}'`);
          return match[1];
        }
      }
      
      // Method 4: Check for theme in selected option text
      if (themeSelector) {
        const selectedOption = themeSelector.selectedOptions[0];
        if (selectedOption && selectedOption.value) {
          console.log(`🔍 ThemeGetBrowser: Got theme from selected option: '${selectedOption.value}'`);
          return selectedOption.value;
        }
      }
      
      console.warn(`⚠️ ThemeGetBrowser: Could not determine current theme using any method`);
      return null;
      
    } catch (error) {
      console.error(`❌ ThemeGetBrowser: Error getting current theme:`, error);
      return null;
    }
  }
  
  private async getThemeManifest(themeName: string): Promise<ThemeManifest | undefined> {
    try {
      // Try to get theme manifest from ThemeRegistry if available
      if (typeof window !== 'undefined' && window.ThemeRegistry) {
        const registry = window.ThemeRegistry;
        if (typeof registry.getTheme === 'function') {
          const manifest = registry.getTheme(themeName);
          if (manifest) {
            console.log(`🔍 ThemeGetBrowser: Got theme manifest for '${themeName}' from ThemeRegistry`);
            return manifest;
          }
        }
      }
      
      // Try to get theme info from dropdown option attributes
      const themeSelector = document.querySelector('#theme-selector') as HTMLSelectElement ||
                           document.querySelector('universe-widget select') as HTMLSelectElement;
      
      if (themeSelector) {
        const option = Array.from(themeSelector.options).find(opt => opt.value === themeName);
        if (option) {
          // Create basic manifest from dropdown option
          const manifest: ThemeManifest = {
            name: themeName,
            displayName: option.textContent || themeName,
            description: option.title || `${option.textContent} theme`,
            category: 'discovered',
            author: 'JTAG Team',
            version: '1.0.0',
            files: ['theme.css'],
            tags: ['discovered'],
            preview: {
              primaryColor: '#000000',
              backgroundColor: '#ffffff', 
              textColor: '#000000'
            }
          };
          
          console.log(`🔍 ThemeGetBrowser: Created basic manifest for '${themeName}' from dropdown`);
          return manifest;
        }
      }
      
      console.warn(`⚠️ ThemeGetBrowser: No manifest found for theme '${themeName}'`);
      return undefined;
      
    } catch (error) {
      console.warn(`⚠️ ThemeGetBrowser: Error getting theme manifest:`, error);
      return undefined;
    }
  }
  
  private async isThemeApplied(themeName: string): Promise<boolean> {
    try {
      // Check if theme style element exists and has content
      const themeStyle = document.head.querySelector(`#jtag-theme-${themeName}`) as HTMLStyleElement;
      
      if (themeStyle && themeStyle.textContent && themeStyle.textContent.length > 0) {
        console.log(`🔍 ThemeGetBrowser: Theme '${themeName}' is applied (style element has ${themeStyle.textContent.length} chars)`);
        return true;
      }
      
      // Check for any theme style element (might be different theme)
      const anyThemeStyle = document.head.querySelector('[id^="jtag-theme-"]') as HTMLStyleElement;
      if (anyThemeStyle) {
        console.log(`🔍 ThemeGetBrowser: Different theme applied: ${anyThemeStyle.id}`);
        return false; // Different theme is applied
      }
      
      console.warn(`⚠️ ThemeGetBrowser: No theme style element found - theme may not be applied`);
      return false;
      
    } catch (error) {
      console.warn(`⚠️ ThemeGetBrowser: Error checking if theme is applied:`, error);
      return false;
    }
  }
}