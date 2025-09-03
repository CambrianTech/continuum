/**
 * ThemeSet Browser Command - Execute theme switching in browser environment
 */

import type { ThemeSetParams, ThemeSetResult } from '../shared/ThemeSetTypes';
import { createThemeSetResult } from '../shared/ThemeSetTypes';
import { EnhancementError } from '../../../../system/core/types/ErrorTypes';
import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { ThemeRegistry } from '../../../../widgets/shared/themes/ThemeTypes';

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
   * SIMPLIFIED: Browser theme switching using existing theme infrastructure
   */
  private async setThemeDirectly(themeName: string, params: ThemeSetParams): Promise<void> {
    console.log(`🎨 ThemeSetBrowser: Switching to theme '${themeName}' using simplified approach`);
    
    try {
      // STEP 1: Check if theme exists in registry
      const themeManifest = ThemeRegistry.getTheme(themeName);
      if (!themeManifest) {
        throw new Error(`Theme '${themeName}' not found in registry`);
      }
      
      // STEP 2: Load ALL theme CSS (base + theme) - SAME AS ThemeWidget.loadAllThemeCSS()
      const combinedCSS = await this.loadAllThemeCSS(themeName, params);
      
      // STEP 3: Inject CSS into document head - SAME AS ThemeWidget.injectThemeIntoDocumentHead()
      await this.injectThemeIntoDocumentHead(combinedCSS, themeName);
      
      console.log('✅ ThemeSetBrowser: Theme switched using simplified approach');
      
    } catch (error) {
      console.error('❌ ThemeSetBrowser: Failed to switch theme:', error);
      throw error;
    }
  }

  /**
   * COPIED EXACTLY FROM ThemeWidget.loadAllThemeCSS() - Load ALL theme CSS (base + current theme)
   */
  private async loadAllThemeCSS(themeName: string, params: ThemeSetParams): Promise<string> {
    try {
      console.log('🎨 ThemeSetBrowser: Loading ALL theme CSS (base + theme)');
      
      // Load base CSS files from themes/base/
      const baseStyles = await this.loadDirectoryStyles('base', params);
      
      // Load theme-specific CSS files from themes/current-theme-name/
      const themeStyles = themeName !== 'base' 
        ? await this.loadDirectoryStyles(themeName, params)
        : '';
      
      // Combine base + theme styles
      const combinedCSS = baseStyles + themeStyles;
      
      console.log(`✅ ThemeSetBrowser: Combined theme CSS loaded (${combinedCSS.length} chars)`);
      return combinedCSS;
      
    } catch (error) {
      console.error('❌ ThemeSetBrowser: Failed to load all theme CSS:', error);
      return '';
    }
  }

  /**
   * COPIED EXACTLY FROM ThemeWidget.loadDirectoryStyles() - Load all CSS files from a theme directory
   */
  private async loadDirectoryStyles(directoryName: string, params: ThemeSetParams): Promise<string> {
    try {
      // Get list of files to load for this directory
      const cssFiles = await this.getDirectoryFiles(directoryName);
      let combinedStyles = '';
      
      for (const fileName of cssFiles) {
        try {
          // Use BaseWidget's protected jtagOperation method - same as loadResource does internally
          const filePath = `widgets/shared/themes/${directoryName}/${fileName}`;
          console.log(`🎨 ThemeSetBrowser: Loading ${filePath} via file/load command`);
          
          // EXACT SAME APPROACH AS ThemeWidget - use JTAG client directly
          console.log(`🎨 ThemeSetBrowser: Loading ${filePath} via JTAG client like ThemeWidget`);
          
          // Get the JTAG client from window (same as BaseWidget.jtagOperation)
          const jtagClient = (window as any).jtag;
          if (!jtagClient || !jtagClient.commands) {
            throw new Error('JTAG client not available - system not ready');
          }
          
          // Call file/load command directly (same as ThemeWidget does)
          const result = await jtagClient.commands['file/load']({
            filepath: filePath
          });
          
          // Handle nested JTAG response structure (same as BaseWidget loadResource)
          const fileData = (result as any).commandResult || result;
          if (result.success && fileData.success && fileData.content) {
            combinedStyles += `\n/* === ${directoryName}/${fileName} === */\n${fileData.content}\n`;
            console.log(`✅ ThemeSetBrowser: Loaded ${directoryName}/${fileName} (${fileData.bytesRead} bytes)`);
          } else {
            console.log(`⚠️ ThemeSetBrowser: ${directoryName}/${fileName} not found - skipping`);
          }
        } catch (error) {
          console.warn(`⚠️ ThemeSetBrowser: Could not load ${directoryName}/${fileName}:`, error);
        }
      }
      
      return combinedStyles;
    } catch (error) {
      console.error(`❌ ThemeSetBrowser: Failed to load directory styles for '${directoryName}':`, error);
      return '';
    }
  }

  /**
   * COPIED EXACTLY FROM ThemeWidget.getDirectoryFiles() - Get list of CSS files for a theme from its manifest
   */
  private async getDirectoryFiles(directoryName: string): Promise<string[]> {
    // Get files from theme manifest in registry
    const themeManifest = ThemeRegistry.getTheme(directoryName);
    if (themeManifest) {
      return themeManifest.files;
    }
    
    // Fallback to standard theme.css if no manifest found
    console.warn(`⚠️ ThemeSetBrowser: No manifest found for theme '${directoryName}', using fallback`);
    return ['theme.css'];
  }

  /**
   * COPIED FROM ThemeWidget.injectThemeIntoDocumentHead() - Inject CSS into document head
   */
  private async injectThemeIntoDocumentHead(combinedCSS: string, themeName: string): Promise<void> {
    try {
      console.log('🎨 ThemeSetBrowser: Injecting theme CSS into document head for global access...');
      console.log('🔧 CLAUDE-DEBUG: combinedCSS length:', combinedCSS.length);
      console.log('🔧 CLAUDE-DEBUG: combinedCSS first 200 chars:', combinedCSS.substring(0, 200));
      
      // Remove existing theme style element
      if (this.themeStyleElement) {
        console.log('🔧 CLAUDE-DEBUG: Removing existing theme style element:', this.themeStyleElement.id);
        this.themeStyleElement.remove();
        this.themeStyleElement = null;
      }
      
      // Create new theme style element and inject into document head
      this.themeStyleElement = document.createElement('style');
      this.themeStyleElement.id = `jtag-theme-${themeName}`;
      this.themeStyleElement.textContent = combinedCSS;
      
      console.log('🔧 CLAUDE-DEBUG: Created style element with id:', this.themeStyleElement.id);
      console.log('🔧 CLAUDE-DEBUG: About to append to document.head...');
      
      document.head.appendChild(this.themeStyleElement);
      
      // Verify the injection worked
      const verifyElement = document.head.querySelector(`#jtag-theme-${themeName}`);
      console.log('🔧 CLAUDE-DEBUG: Verification - element exists in document head:', !!verifyElement);
      console.log('🔧 CLAUDE-DEBUG: Verification - element content length:', verifyElement?.textContent?.length || 0);
      
      console.log(`✅ ThemeSetBrowser: Theme '${themeName}' CSS injected into document head (${combinedCSS.length} chars)`);
      
      // Dispatch theme change event
      document.dispatchEvent(new CustomEvent('theme-changed', {
        detail: { themeName: themeName },
        bubbles: true
      }));
      
    } catch (error) {
      console.error('❌ ThemeSetBrowser: Failed to inject theme CSS into document head:', error);
      console.error('🔧 CLAUDE-DEBUG: Error stack:', (error as Error).stack);
      throw error;
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