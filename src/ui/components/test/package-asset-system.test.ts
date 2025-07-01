/**
 * Package.json Asset System Tests
 * Tests the pure package.json-based asset discovery and serving system
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';

// Mock fetch for testing
global.fetch = jest.fn();

// Mock widget class for testing
class TestWidget {
  static name = 'TestWidget';
  
  static getBasePath(): string {
    const className = this.name.replace('Widget', '');
    return `/dist/ui/components/${className}`;
  }
  
  static async getWidgetFiles(): Promise<string[]> {
    try {
      const basePath = this.getBasePath().replace('/dist/', '/src/');
      const packagePath = `${basePath}/package.json`;
      
      const response = await fetch(packagePath);
      if (!response.ok) {
        return [];
      }
      
      const packageData = await response.json();
      return packageData.files || [];
    } catch (error) {
      return [];
    }
  }
  
  static async getWidgetAssets(): Promise<string[]> {
    const widgetFiles = await this.getWidgetFiles();
    const assets = widgetFiles.filter(file => !file.endsWith('.ts'));
    return assets.map(file => `${this.getBasePath()}/${file}`);
  }
}

describe('Package.json Asset System', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  describe('getBasePath()', () => {
    it('should generate correct paths from class names', () => {
      class ChatWidget extends TestWidget {
        static name = 'ChatWidget';
      }
      
      class SidebarWidget extends TestWidget {
        static name = 'SidebarWidget';
      }
      
      expect(ChatWidget.getBasePath()).toBe('/dist/ui/components/Chat');
      expect(SidebarWidget.getBasePath()).toBe('/dist/ui/components/Sidebar');
    });
    
    it('should handle shared components', () => {
      class BaseWidget extends TestWidget {
        static name = 'BaseWidget';
        
        static getBasePath(): string {
          const className = this.name.replace('Widget', '');
          if (className === 'Base' || className === 'Interactive' || this.name.includes('BaseWidget')) {
            return '/dist/ui/components/shared';
          }
          return `/dist/ui/components/${className}`;
        }
      }
      
      expect(BaseWidget.getBasePath()).toBe('/dist/ui/components/shared');
    });
  });

  describe('getWidgetFiles()', () => {
    it('should read files array from package.json', async () => {
      const mockPackageData = {
        name: '@continuum/test-widget',
        files: ['TestWidget.ts', 'TestWidget.css', 'icon.svg', 'sound.mp3']
      };
      
      (fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockPackageData)
      });
      
      const files = await TestWidget.getWidgetFiles();
      
      expect(fetch).toHaveBeenCalledWith('/src/ui/components/Test/package.json');
      expect(files).toEqual(['TestWidget.ts', 'TestWidget.css', 'icon.svg', 'sound.mp3']);
    });
    
    it('should handle missing package.json', async () => {
      (fetch as jest.Mock).mockResolvedValueOnce({
        ok: false,
        status: 404
      });
      
      const files = await TestWidget.getWidgetFiles();
      
      expect(files).toEqual([]);
    });
    
    it('should handle missing files array', async () => {
      const mockPackageData = {
        name: '@continuum/test-widget'
        // No files array
      };
      
      (fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockPackageData)
      });
      
      const files = await TestWidget.getWidgetFiles();
      
      expect(files).toEqual([]);
    });
  });

  describe('getWidgetAssets()', () => {
    it('should filter out TypeScript files and generate proper URLs', async () => {
      const mockPackageData = {
        files: ['TestWidget.ts', 'TestWidget.css', 'icon.svg', 'sound.mp3', 'font.woff2']
      };
      
      (fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockPackageData)
      });
      
      const assets = await TestWidget.getWidgetAssets();
      
      expect(assets).toEqual([
        '/dist/ui/components/Test/TestWidget.css',
        '/dist/ui/components/Test/icon.svg',
        '/dist/ui/components/Test/sound.mp3',
        '/dist/ui/components/Test/font.woff2'
      ]);
    });
    
    it('should handle widgets with only TypeScript files', async () => {
      const mockPackageData = {
        files: ['TestWidget.ts', 'types.ts', 'interfaces.ts']
      };
      
      (fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockPackageData)
      });
      
      const assets = await TestWidget.getWidgetAssets();
      
      expect(assets).toEqual([]);
    });
    
    it('should handle empty files array', async () => {
      const mockPackageData = {
        files: []
      };
      
      (fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockPackageData)
      });
      
      const assets = await TestWidget.getWidgetAssets();
      
      expect(assets).toEqual([]);
    });
  });

  describe('Asset Type Support', () => {
    it('should support all common asset types', async () => {
      const mockPackageData = {
        files: [
          'Widget.ts',        // Filtered out
          'styles.css',       // CSS
          'icon.svg',         // Vector image
          'photo.jpg',        // Raster image
          'video.mp4',        // Video
          'audio.mp3',        // Audio
          'font.woff2',       // Font
          'data.json',        // Data
          'template.html',    // HTML template
          'config.xml'        // Configuration
        ]
      };
      
      (fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockPackageData)
      });
      
      const assets = await TestWidget.getWidgetAssets();
      
      expect(assets).toEqual([
        '/dist/ui/components/Test/styles.css',
        '/dist/ui/components/Test/icon.svg',
        '/dist/ui/components/Test/photo.jpg',
        '/dist/ui/components/Test/video.mp4',
        '/dist/ui/components/Test/audio.mp3',
        '/dist/ui/components/Test/font.woff2',
        '/dist/ui/components/Test/data.json',
        '/dist/ui/components/Test/template.html',
        '/dist/ui/components/Test/config.xml'
      ]);
    });
  });

  describe('Error Handling', () => {
    it('should handle fetch errors gracefully', async () => {
      (fetch as jest.Mock).mockRejectedValueOnce(new Error('Network error'));
      
      const files = await TestWidget.getWidgetFiles();
      const assets = await TestWidget.getWidgetAssets();
      
      expect(files).toEqual([]);
      expect(assets).toEqual([]);
    });
    
    it('should handle invalid JSON gracefully', async () => {
      (fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: () => Promise.reject(new Error('Invalid JSON'))
      });
      
      const files = await TestWidget.getWidgetFiles();
      
      expect(files).toEqual([]);
    });
  });

  describe('Real Widget Examples', () => {
    it('should work with ChatWidget package.json', async () => {
      const mockChatPackage = {
        name: '@continuum/chat-widget',
        files: [
          'ChatWidget.ts',
          'ChatWidget.css',
          'chat-icon.svg',
          'notification.mp3',
          'emoji-sprites.png'
        ]
      };
      
      class ChatWidget extends TestWidget {
        static name = 'ChatWidget';
      }
      
      (fetch as jest.Mock).mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve(mockChatPackage)
      });
      
      const assets = await ChatWidget.getWidgetAssets();
      
      expect(assets).toEqual([
        '/dist/ui/components/Chat/ChatWidget.css',
        '/dist/ui/components/Chat/chat-icon.svg',
        '/dist/ui/components/Chat/notification.mp3',
        '/dist/ui/components/Chat/emoji-sprites.png'
      ]);
    });
  });
});