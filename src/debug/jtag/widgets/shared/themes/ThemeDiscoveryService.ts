/**
 * Theme Discovery Service - Dynamic theme loading and discovery
 * Works like command/widget/daemon discovery - no hardcoded themes
 */

import { ThemeManifest, ThemeRegistry, ThemeLoadResult } from './ThemeTypes';

export class ThemeDiscoveryService {
  private baseWidget: any; // Reference to BaseWidget for file operations
  private themesPath = 'widgets/shared/themes';

  constructor(baseWidget: any) {
    this.baseWidget = baseWidget;
  }

  /**
   * Discover all available themes by scanning theme directories
   * Similar to how commands/widgets/daemons are discovered
   */
  async discoverThemes(): Promise<ThemeManifest[]> {
    console.log('🎨 ThemeDiscoveryService: Starting theme discovery...');
    
    try {
      // Get list of theme directories (similar to widget discovery)
      const themeDirectories = await this.getThemeDirectories();
      
      // Load manifest for each theme
      const themes: ThemeManifest[] = [];
      for (const themeDir of themeDirectories) {
        try {
          const manifest = await this.loadThemeManifest(themeDir);
          if (manifest) {
            themes.push(manifest);
            ThemeRegistry.registerTheme(manifest);
            console.log(`✅ ThemeDiscoveryService: Registered theme '${manifest.name}' - ${manifest.displayName}`);
          }
        } catch (error) {
          console.warn(`⚠️ ThemeDiscoveryService: Could not load theme '${themeDir}':`, error);
        }
      }

      console.log(`🎨 ThemeDiscoveryService: Discovered ${themes.length} themes`);
      return themes;
    } catch (error) {
      console.error('❌ ThemeDiscoveryService: Theme discovery failed:', error);
      return [];
    }
  }

  /**
   * Load a specific theme's CSS content
   */
  async loadTheme(themeName: string): Promise<ThemeLoadResult> {
    console.log(`🎨 ThemeDiscoveryService: Loading theme '${themeName}'`);
    
    try {
      const manifest = ThemeRegistry.getTheme(themeName);
      if (!manifest) {
        return {
          success: false,
          themeName,
          cssContent: '',
          error: `Theme '${themeName}' not found in registry`
        };
      }

      let combinedCSS = '';
      
      // Load each CSS file for this theme
      for (const filename of manifest.files) {
        try {
          const filePath = `${this.themesPath}/${themeName}/${filename}`;
          console.log(`🎨 ThemeDiscoveryService: Loading ${filePath}`);
          
          const result = await this.baseWidget.executeCommand('file/load', {
            filepath: filePath
          });
          
          const fileData = (result as any).commandResult || result;
          if (result.success && fileData.success && fileData.content) {
            combinedCSS += `\\n/* === ${themeName}/${filename} === */\\n${fileData.content}\\n`;
            console.log(`✅ ThemeDiscoveryService: Loaded ${filename} (${fileData.bytesRead} bytes)`);
          } else {
            console.warn(`⚠️ ThemeDiscoveryService: Could not load ${filename}`);
          }
        } catch (error) {
          console.warn(`⚠️ ThemeDiscoveryService: Error loading ${filename}:`, error);
        }
      }

      return {
        success: true,
        themeName,
        cssContent: combinedCSS
      };
    } catch (error) {
      return {
        success: false,
        themeName,
        cssContent: '',
        error: `Failed to load theme '${themeName}': ${error}`
      };
    }
  }

  /**
   * Get list of theme directories by scanning the themes folder
   * TODO: Implement dynamic directory scanning when file/list command is available
   */
  private async getThemeDirectories(): Promise<string[]> {
    // Future: Dynamic directory scanning would look like this:
    // const result = await this.baseWidget.executeCommand('file/list', { path: this.themesPath });
    // return result.directories || [];
    
    // For now, discover by attempting to load manifests for potential themes
    // This is more dynamic than hardcoding - we try common theme names
    // and only return those that have valid manifests
    const potentialThemes = [
      'base',
      'light', 
      'cyberpunk',
      'retro-mac',
      'monochrome',
      'classic',
      'dark',
      'high-contrast',
      'minimal',
      'neon'
    ];
    
    const validThemes: string[] = [];
    
    // Test each potential theme by trying to load its manifest
    for (const themeName of potentialThemes) {
      try {
        const manifest = await this.loadThemeManifest(themeName);
        if (manifest) {
          validThemes.push(themeName);
        }
      } catch (error) {
        // Theme doesn't exist or has invalid manifest - skip silently
      }
    }
    
    console.log(`🎨 ThemeDiscoveryService: Found ${validThemes.length} valid themes with manifests`);
    return validThemes;
  }

  /**
   * Load theme manifest (theme.json) for a specific theme
   */
  private async loadThemeManifest(themeName: string): Promise<ThemeManifest | null> {
    try {
      const manifestPath = `${this.themesPath}/${themeName}/theme.json`;
      console.log(`🎨 ThemeDiscoveryService: Loading manifest ${manifestPath}`);
      
      const result = await this.baseWidget.executeCommand('file/load', {
        filepath: manifestPath
      });
      
      const fileData = (result as any).commandResult || result;
      if (result.success && fileData.success && fileData.content) {
        const manifest: ThemeManifest = JSON.parse(fileData.content);
        console.log(`✅ ThemeDiscoveryService: Loaded manifest for '${manifest.name}'`);
        return manifest;
      } else {
        console.warn(`⚠️ ThemeDiscoveryService: No manifest found for '${themeName}'`);
        return null;
      }
    } catch (error) {
      console.warn(`⚠️ ThemeDiscoveryService: Error loading manifest for '${themeName}':`, error);
      return null;
    }
  }
}