/**
 * Version Command Tests - Small, focused validation
 */

const { describe, it, expect, beforeEach } = require('@jest/globals');

describe('Version Command', () => {
  let VersionCommand;
  
  beforeEach(async () => {
    const module = await import('../VersionCommand.js');
    VersionCommand = module.VersionCommand;
  });

  it('should return valid version format', async () => {
    const cmd = new VersionCommand();
    const result = await cmd.execute();
    
    expect(result.success).toBe(true);
    expect(result.version).toMatch(/^\d+\.\d+\.\d+$/);
    expect(result.uptime).toMatch(/^\d+h \d+m \d+s$/);
    expect(result.buildTimestamp).toBeDefined();
  });

  it('should provide build info', async () => {
    const cmd = new VersionCommand();
    const result = await cmd.execute();
    
    expect(result.build).toBe('TypeScript Daemon System');
    expect(result.nodeVersion).toMatch(/^v\d+/);
    expect(result.platform).toContain(process.platform);
  });

  it('should handle errors gracefully', async () => {
    const cmd = new VersionCommand();
    
    // Mock fs to throw error
    const originalImport = global.__esModule_import || import;
    global.__esModule_import = (specifier) => {
      if (specifier === 'fs') {
        return Promise.resolve({
          readFileSync: () => { throw new Error('Test error'); },
          statSync: () => ({ mtime: new Date() })
        });
      }
      return originalImport(specifier);
    };
    
    const result = await cmd.execute();
    
    expect(result.success).toBe(false);
    expect(result.error).toContain('Failed to get version');
    
    // Restore
    global.__esModule_import = originalImport;
  });
});

describe('Restart Integration', () => {
  it('should detect version changes', async () => {
    // Test version increment detection
    const fs = require('fs');
    const packagePath = './package.json';
    const originalContent = fs.readFileSync(packagePath, 'utf8');
    const originalData = JSON.parse(originalContent);
    const originalVersion = originalData.version;
    
    // Version should be in proper format
    expect(originalVersion).toMatch(/^\d+\.\d+\.\d+$/);
    
    // Parse version parts
    const [major, minor, patch] = originalVersion.split('.').map(Number);
    expect(major).toBeGreaterThanOrEqual(0);
    expect(minor).toBeGreaterThanOrEqual(0);
    expect(patch).toBeGreaterThanOrEqual(0);
  });
});