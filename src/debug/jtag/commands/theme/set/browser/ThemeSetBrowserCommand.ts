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
      await this.setThemeDirectly(themeName);
      
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
  private async setThemeDirectly(themeName: string): Promise<void> {
    console.log(`🎨 ThemeSetBrowser: Switching to theme '${themeName}' using simplified approach`);
    
    try {
      // STEP 1: Check if theme exists in registry
      const themeManifest = ThemeRegistry.getTheme(themeName);
      if (!themeManifest) {
        throw new Error(`Theme '${themeName}' not found in registry`);
      }
      
      // STEP 2: For now, create basic theme CSS (will improve with proper file loading later)
      const basicThemeCSS = this.createBasicThemeCSS(themeName, themeManifest);
      
      // STEP 3: Inject CSS into document head (browser responsibility)
      await this.injectThemeIntoDocumentHead(basicThemeCSS, themeName);
      
      console.log('✅ ThemeSetBrowser: Theme switched using simplified approach');
      
    } catch (error) {
      console.error('❌ ThemeSetBrowser: Failed to switch theme:', error);
      throw error;
    }
  }

  /**
   * Create basic theme CSS using theme manifest (temporary until proper file loading)
   */
  private createBasicThemeCSS(themeName: string, manifest: any): string {
    const primaryColor = manifest.preview?.primaryColor || '#007acc';
    const backgroundColor = manifest.preview?.backgroundColor || '#1e1e1e';
    const textColor = manifest.preview?.textColor || '#ffffff';
    
    return `
/* Theme: ${themeName} - ${manifest.displayName} */
:root {
  --theme-primary: ${primaryColor};
  --theme-background: ${backgroundColor};
  --theme-text: ${textColor};
  --theme-name: "${themeName}";
}

/* Apply theme colors to common elements */
body {
  background-color: var(--theme-background);
  color: var(--theme-text);
}

.continuum-sidebar {
  background-color: var(--theme-background);
  border-right: 1px solid var(--theme-primary);
}

chat-widget {
  --chat-background: var(--theme-background);
  --chat-text: var(--theme-text);
  --chat-accent: var(--theme-primary);
}
`;
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