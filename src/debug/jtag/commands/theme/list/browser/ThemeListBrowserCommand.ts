/**
 * ThemeList Browser Command - List available themes in browser environment
 */

import type { ThemeListParams, ThemeListResult } from '../shared/ThemeListTypes';
import type { ThemeManifest } from '../../shared/ThemeTypes';
import { createThemeListResult } from '../shared/ThemeListTypes';
import { EnhancementError } from '../../../../system/core/types/ErrorTypes';

export class ThemeListBrowserCommand {
  async execute(params: ThemeListParams): Promise<ThemeListResult> {
    try {
      console.log(`🎨 ThemeListBrowser: Listing available themes${params.category ? ` in category '${params.category}'` : ''}`);
      
      // Get themes and their manifests
      const { themes, manifests } = await this.getAvailableThemes();
      
      // Filter by category if specified
      let filteredThemes = themes;
      let filteredManifests = manifests;
      
      if (params.category && manifests) {
        filteredManifests = manifests.filter(manifest => manifest.category === params.category);
        filteredThemes = filteredManifests.map(manifest => manifest.name);
      }
      
      // Get unique categories
      const categories = manifests 
        ? Array.from(new Set(manifests.map(manifest => manifest.category))).sort()
        : [];
      
      // Get current theme
      const currentTheme = await this.getCurrentTheme();
      
      console.log(`✅ ThemeListBrowser: Found ${filteredThemes.length} themes${params.category ? ` in category '${params.category}'` : ''}`);
      console.log(`🎨 ThemeListBrowser: Available themes: ${filteredThemes.join(', ')}`);
      console.log(`📂 ThemeListBrowser: Categories: ${categories.join(', ')}`);
      console.log(`🎯 ThemeListBrowser: Current theme: ${currentTheme}`);
      
      return createThemeListResult(params.context, params.sessionId, {
        success: true,
        themes: filteredThemes,
        categories,
        currentTheme,
        ...(params.includeManifests && filteredManifests && { manifests: filteredManifests })
      });
      
    } catch (error) {
      const jtagError = new EnhancementError(
        'theme-list',
        `Error listing themes: ${error}`
      );
      
      return createThemeListResult(params.context, params.sessionId, {
        success: false,
        themes: [],
        categories: [],
        currentTheme: 'unknown',
        error: jtagError
      });
    }
  }
  
  private async getAvailableThemes(): Promise<{
    themes: string[];
    manifests?: ThemeManifest[];
  }> {
    try {
      // Method 1: Get from ThemeRegistry if available (preferred)
      if (typeof window !== 'undefined' && (window as any).ThemeRegistry) {
        const registry = (window as any).ThemeRegistry;
        if (typeof registry.getAllThemes === 'function') {
          const manifests = registry.getAllThemes() as ThemeManifest[];
          const themes = manifests.map(manifest => manifest.name);
          console.log(`🔍 ThemeListBrowser: Got ${themes.length} themes from ThemeRegistry`);
          return { themes, manifests };
        }
      }
      
      // Method 2: Get from theme selector dropdown
      const themeSelector = document.querySelector('#theme-selector') as HTMLSelectElement ||
                           document.querySelector('theme-widget select') as HTMLSelectElement;
      
      if (themeSelector && themeSelector.options) {
        const themes: string[] = [];
        const manifests: ThemeManifest[] = [];
        
        Array.from(themeSelector.options).forEach(option => {
          if (option.value && option.value !== '') {
            themes.push(option.value);
            
            // Create basic manifest from option
            const manifest: ThemeManifest = {
              name: option.value,
              displayName: option.textContent || option.value,
              description: option.title || `${option.textContent} theme`,
              category: 'discovered',
              author: 'JTAG Team',
              version: '1.0.0',
              files: ['theme.css'],
              tags: ['dropdown-discovered'],
              preview: {
                primaryColor: '#000000',
                backgroundColor: '#ffffff',
                textColor: '#000000'
              }
            };
            manifests.push(manifest);
          }
        });
        
        console.log(`🔍 ThemeListBrowser: Got ${themes.length} themes from dropdown`);
        return { themes, manifests };
      }
      
      // Method 3: Fallback to known themes
      console.warn(`⚠️ ThemeListBrowser: Using fallback theme list`);
      const fallbackThemes = [
        'base',
        'light', 
        'cyberpunk',
        'retro-mac',
        'monochrome',
        'classic'
      ];
      
      const fallbackManifests: ThemeManifest[] = [
        {
          name: 'base',
          displayName: 'Base - Dark Cyberpunk',
          description: 'Default cyberpunk theme',
          category: 'dark',
          author: 'JTAG Team',
          version: '1.0.0',
          files: ['base.css', 'theme.css'],
          tags: ['dark', 'cyberpunk', 'default'],
          preview: { primaryColor: '#00ffff', backgroundColor: '#0f1419', textColor: '#ffffff' }
        },
        {
          name: 'light',
          displayName: 'Light - Clean Professional',
          description: 'Clean light theme for professional use',
          category: 'light',
          author: 'JTAG Team',
          version: '1.0.0',
          files: ['theme.css'],
          tags: ['light', 'professional', 'clean'],
          preview: { primaryColor: '#007acc', backgroundColor: '#ffffff', textColor: '#333333' }
        },
        {
          name: 'cyberpunk',
          displayName: 'Cyberpunk - Neon Future',
          description: 'Bright neon cyberpunk theme',
          category: 'dark',
          author: 'JTAG Team',
          version: '1.0.0',
          files: ['theme.css'],
          tags: ['dark', 'cyberpunk', 'neon'],
          preview: { primaryColor: '#ff00ff', backgroundColor: '#000000', textColor: '#00ffff' }
        },
        {
          name: 'retro-mac',
          displayName: 'Retro Mac - System 11',
          description: 'Classic Mac OS System 11 aesthetics',
          category: 'retro',
          author: 'JTAG Team',
          version: '1.0.0',
          files: ['theme.css'],
          tags: ['retro', 'mac', 'classic'],
          preview: { primaryColor: '#000000', backgroundColor: '#c0c0c0', textColor: '#000000' }
        },
        {
          name: 'monochrome',
          displayName: 'Monochrome - High Contrast',
          description: 'High contrast black and white for accessibility',
          category: 'accessibility',
          author: 'JTAG Team',
          version: '1.0.0',
          files: ['theme.css'],
          tags: ['accessibility', 'high-contrast', 'monochrome'],
          preview: { primaryColor: '#ffffff', backgroundColor: '#000000', textColor: '#ffffff' }
        },
        {
          name: 'classic',
          displayName: 'Classic - Professional',
          description: 'Traditional professional interface',
          category: 'professional',
          author: 'JTAG Team',
          version: '1.0.0',
          files: ['theme.css'],
          tags: ['professional', 'traditional', 'classic'],
          preview: { primaryColor: '#3498db', backgroundColor: '#ecf0f1', textColor: '#2c3e50' }
        }
      ];
      
      return { themes: fallbackThemes, manifests: fallbackManifests };
      
    } catch (error) {
      console.error(`❌ ThemeListBrowser: Error getting available themes:`, error);
      return { themes: [] };
    }
  }
  
  private async getCurrentTheme(): Promise<string> {
    try {
      // Try ThemeWidget first
      const themeWidget = document.querySelector('theme-widget') as any;
      if (themeWidget && typeof themeWidget.getCurrentTheme === 'function') {
        const theme = themeWidget.getCurrentTheme();
        if (theme) return theme;
      }
      
      // Try dropdown selector
      const themeSelector = document.querySelector('#theme-selector') as HTMLSelectElement ||
                           document.querySelector('theme-widget select') as HTMLSelectElement;
      
      if (themeSelector && themeSelector.value) {
        return themeSelector.value;
      }
      
      // Try theme style element
      const themeStyle = document.head.querySelector('[id^="jtag-theme-"]') as HTMLStyleElement;
      if (themeStyle) {
        const match = themeStyle.id.match(/^jtag-theme-(.+)$/);
        if (match && match[1]) {
          return match[1];
        }
      }
      
      return 'unknown';
      
    } catch (error) {
      console.warn(`⚠️ ThemeListBrowser: Error getting current theme:`, error);
      return 'unknown';
    }
  }
}