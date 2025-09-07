/**
 * ThemeSet Browser Command - Execute theme switching in browser environment
 */

import type { ThemeSetParams, ThemeSetResult } from '../shared/ThemeSetTypes';
import { createThemeSetResult } from '../shared/ThemeSetTypes';
import { EnhancementError } from '../../../../system/core/types/ErrorTypes';
import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';

export class ThemeSetBrowserCommand extends CommandBase<ThemeSetParams, ThemeSetResult> {
  private themeStyleElement: HTMLStyleElement | null = null;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('theme-set', context, subpath, commander);
  }

  async execute(params: ThemeSetParams): Promise<ThemeSetResult> {
    // Handle CLI positional arguments - if themeName is undefined, use first positional argument
    let themeName = params.themeName;
    if (!themeName && (params as any)._positional && (params as any)._positional.length > 0) {
      themeName = (params as any)._positional[0];
      console.log(`🔧 ThemeSetBrowser: Using positional argument '${themeName}' as theme name`);
    }
    
    try {
      console.log(`🎨 ThemeSetBrowser: Setting theme to '${themeName}' using direct theme switching`);
      
      // Get current theme before switching
      const previousTheme = await this.getCurrentTheme();
      
      // COPY THE WORKING THEME SWITCHING CODE FROM ThemeWidget.setTheme()
      await this.setThemeDirectly(themeName, params);
      
      return createThemeSetResult(params.context, params.sessionId, {
        success: true,
        themeName: themeName,
        previousTheme,
        applied: true
      });
      
    } catch (error) {
      const jtagError = new EnhancementError(
        'theme-set',
        `Failed to set theme '${themeName}': ${error}`
      );
      
      return createThemeSetResult(params.context, params.sessionId, {
        success: false,
        themeName: themeName,
        applied: false,
        error: jtagError
      });
    }
  }

  /**
   * Simple theme switching - delegate to ThemeWidget if available
   */
  private async setThemeDirectly(themeName: string, params: ThemeSetParams): Promise<void> {
    console.log(`🎨 ThemeSetBrowser: Setting theme '${themeName}'`);
    
    try {
      // Try to delegate to existing ThemeWidget
      const themeWidget = document.querySelector('theme-widget') as any;
      if (themeWidget && typeof themeWidget.setTheme === 'function') {
        console.log('🎨 ThemeSetBrowser: Delegating to ThemeWidget');
        await themeWidget.setTheme(themeName);
        console.log('✅ ThemeSetBrowser: Theme set via ThemeWidget delegation');
        return;
      }
      
      // Fallback - simple CSS loading
      console.log('🎨 ThemeSetBrowser: No ThemeWidget found, using fallback');
      const baseCSS = await this.loadThemeFile('base/theme.css');
      const themeCSS = themeName !== 'base' ? await this.loadThemeFile(`${themeName}/theme.css`) : '';
      
      const combinedCSS = baseCSS + '\n' + themeCSS;
      this.injectCSS(combinedCSS, themeName);
      
      console.log(`✅ ThemeSetBrowser: Theme '${themeName}' applied`);
      
    } catch (error) {
      console.error('❌ ThemeSetBrowser: Failed to switch theme:', error);
      throw error;
    }
  }

  /**
   * Load a theme CSS file
   */
  private async loadThemeFile(filename: string): Promise<string> {
    try {
      const filePath = `widgets/shared/themes/${filename}`;
      const jtagClient = (window as any).jtag;
      if (!jtagClient?.commands) {
        throw new Error('JTAG client not available');
      }
      
      const result = await jtagClient.commands['file/load']({ filepath: filePath });
      const fileData = (result as any).commandResult || result;
      
      if (result.success && fileData.success && fileData.content) {
        return fileData.content;
      }
      return '';
    } catch (error) {
      console.warn(`⚠️ Could not load theme file ${filename}:`, error);
      return '';
    }
  }
  
  /**
   * Simple CSS injection
   */
  private injectCSS(css: string, themeName: string): void {
    // Remove existing theme style
    if (this.themeStyleElement) {
      this.themeStyleElement.remove();
    }
    
    // Create and inject new style
    this.themeStyleElement = document.createElement('style');
    this.themeStyleElement.id = `jtag-theme-${themeName}`;
    this.themeStyleElement.textContent = css;
    document.head.appendChild(this.themeStyleElement);
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