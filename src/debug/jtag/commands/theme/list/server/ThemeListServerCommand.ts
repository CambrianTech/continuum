/**
 * ThemeList Server Command - Handle theme listing on server side (file system approach)
 */

import type { ThemeListParams, ThemeListResult } from '../shared/ThemeListTypes';
import type { ThemeManifest } from '../../shared/ThemeTypes';
import { createThemeListResult } from '../shared/ThemeListTypes';
import { EnhancementError } from '../../../../system/core/types/ErrorTypes';
import * as fs from 'fs';
import * as path from 'path';

export class ThemeListServerCommand {
  async execute(params: ThemeListParams): Promise<ThemeListResult> {
    try {
      console.log(`🎨 ThemeListServer: Listing themes from file system${params.category ? ` in category '${params.category}'` : ''}`);
      
      // Get themes from file system
      const { themes, manifests } = await this.getThemesFromFileSystem();
      
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
      
      console.log(`✅ ThemeListServer: Found ${filteredThemes.length} themes from file system`);
      console.log(`🎨 ThemeListServer: Available themes: ${filteredThemes.join(', ')}`);
      console.log(`📂 ThemeListServer: Categories: ${categories.join(', ')}`);
      
      return createThemeListResult(params.context, params.sessionId, {
        success: true,
        themes: filteredThemes,
        categories,
        currentTheme: 'server-unknown', // Server can't determine active browser theme
        ...(params.includeManifests && filteredManifests && { manifests: filteredManifests })
      });
      
    } catch (error) {
      const jtagError = new EnhancementError(
        'theme-list-server',
        `Error listing themes from file system: ${error}`
      );
      
      return createThemeListResult(params.context, params.sessionId, {
        success: false,
        themes: [],
        categories: [],
        currentTheme: 'server-error',
        error: jtagError
      });
    }
  }
  
  private async getThemesFromFileSystem(): Promise<{
    themes: string[];
    manifests?: ThemeManifest[];
  }> {
    try {
      const themesPath = path.join(process.cwd(), 'widgets', 'shared', 'themes');
      
      if (!fs.existsSync(themesPath)) {
        console.warn(`⚠️ ThemeListServer: Themes directory not found: ${themesPath}`);
        return this.getFallbackThemes();
      }
      
      const entries = fs.readdirSync(themesPath, { withFileTypes: true });
      const themeDirectories = entries
        .filter(entry => entry.isDirectory())
        .map(entry => entry.name);
      
      console.log(`🔍 ThemeListServer: Found theme directories: ${themeDirectories.join(', ')}`);
      
      const themes: string[] = [];
      const manifests: ThemeManifest[] = [];
      
      for (const themeDir of themeDirectories) {
        try {
          const manifestPath = path.join(themesPath, themeDir, 'theme.json');
          
          if (fs.existsSync(manifestPath)) {
            const manifestContent = fs.readFileSync(manifestPath, 'utf8');
            const manifest: ThemeManifest = JSON.parse(manifestContent);
            
            themes.push(manifest.name);
            manifests.push(manifest);
            console.log(`✅ ThemeListServer: Loaded manifest for '${manifest.name}'`);
          } else {
            // Create basic manifest for directory without manifest file
            themes.push(themeDir);
            const basicManifest: ThemeManifest = {
              name: themeDir,
              displayName: themeDir.charAt(0).toUpperCase() + themeDir.slice(1),
              description: `${themeDir} theme`,
              category: 'discovered',
              author: 'JTAG Team',
              version: '1.0.0',
              files: ['theme.css'],
              tags: ['file-system-discovered'],
              preview: {
                primaryColor: '#000000',
                backgroundColor: '#ffffff',
                textColor: '#000000'
              }
            };
            manifests.push(basicManifest);
            console.log(`🔍 ThemeListServer: Created basic manifest for '${themeDir}'`);
          }
        } catch (error) {
          console.warn(`⚠️ ThemeListServer: Error processing theme '${themeDir}':`, error);
        }
      }
      
      return { themes, manifests };
      
    } catch (error) {
      console.error(`❌ ThemeListServer: Error reading themes from file system:`, error);
      return this.getFallbackThemes();
    }
  }
  
  private getFallbackThemes(): { themes: string[]; manifests: ThemeManifest[] } {
    console.warn(`⚠️ ThemeListServer: Using fallback theme list`);
    
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
  }
}