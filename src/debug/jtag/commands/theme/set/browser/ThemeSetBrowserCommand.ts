/**
 * ThemeSet Browser Command - Execute theme switching in browser environment
 */

import type { ThemeSetParams, ThemeSetResult } from '../shared/ThemeSetTypes';
import { createThemeSetResult } from '../shared/ThemeSetTypes';
import { EnhancementError } from '../../../../system/core/types/ErrorTypes';
import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/client/shared/Commands';
import type { DataListResult } from '../../../../commands/data/list/shared/DataListTypes';
import type { UserStateEntity } from '../../../../system/data/entities/UserStateEntity';

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
   * Theme switching with persistence - delegate to ThemeWidget if available, otherwise use persistence logic
   */
  private async setThemeDirectly(themeName: string, params: ThemeSetParams): Promise<void> {
    console.log(`🎨 ThemeSetBrowser: Setting theme '${themeName}' with persistence`);

    try {
      // Try to delegate to existing ThemeWidget
      const themeWidget = document.querySelector('theme-widget') as any;
      if (themeWidget && typeof themeWidget.setTheme === 'function') {
        console.log('🎨 ThemeSetBrowser: Delegating to ThemeWidget (includes persistence)');
        await themeWidget.setTheme(themeName);
        console.log('✅ ThemeSetBrowser: Theme set via ThemeWidget delegation');
        return;
      }

      // Fallback - apply theme AND save to UserState for persistence
      console.log('🎨 ThemeSetBrowser: No ThemeWidget found, applying theme with persistence');

      // 1. Load and apply CSS
      const baseCSS = await this.loadThemeFile('base/theme.css');
      const themeCSS = themeName !== 'base' ? await this.loadThemeFile(`${themeName}/theme.css`) : '';

      const combinedCSS = baseCSS + '\n' + themeCSS;
      this.injectCSS(combinedCSS, themeName);

      // 2. Save theme preference to UserState for persistence (same logic as ThemeWidget)
      await this.saveThemeToUserState(themeName);

      console.log(`✅ ThemeSetBrowser: Theme '${themeName}' applied and saved to UserState`);

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

  /**
   * Save theme preference to UserState for persistence
   * (Same logic as ThemeWidget - prototype for future State.save<EntityType>())
   */
  private async saveThemeToUserState(themeName: string): Promise<void> {
    try {
      console.log(`🔧 ThemeSetBrowser: Saving theme '${themeName}' to UserState`);

      // Get current user ID from session context
      const sessionInfo = await Commands.execute('session/create', {});
      const userId = (sessionInfo as { userId?: string })?.userId;

      if (!userId) {
        console.warn('⚠️ ThemeSetBrowser: No user ID available, cannot save theme preference');
        return;
      }

      // Find the user's UserState to update theme preference
      const userStates = await Commands.execute('data/list', {
        collection: 'UserState',
        filter: {
          userId: userId
        }
      }) as DataListResult<UserStateEntity>;

      if (userStates.success && userStates.items && userStates.items.length > 0) {
        const userState = userStates.items[0];

        // Update the theme in preferences field using proper typing
        const updatedPreferences = {
          ...userState.preferences,
          theme: themeName
        };

        await Commands.execute('data/update', {
          collection: 'UserState',
          id: userState.id,
          data: {
            preferences: updatedPreferences,
            updatedAt: new Date().toISOString()
          }
        });

        console.log(`✅ ThemeSetBrowser: Theme '${themeName}' saved to UserState`);
      } else {
        console.warn('⚠️ ThemeSetBrowser: No UserState found for theme persistence');
      }

    } catch (error) {
      console.error('❌ ThemeSetBrowser: Failed to save theme to UserState:', error);
    }
  }
}