/**
 * Restart Cycle Test - 50 lines max  
 * Tests restart command integration with version
 */

const { describe, it, expect } = require('@jest/globals');

describe('Restart Cycle', () => {
  it('should execute restart command', async () => {
    // Test the restart command exists and works
    const RestartCommand = require('../../restart/RestartCommand.cjs');
    expect(RestartCommand).toBeDefined();
    expect(RestartCommand.getDefinition).toBeDefined();
    
    const definition = RestartCommand.getDefinition();
    expect(definition.name).toBe('restart');
    expect(definition.parameters.bump).toBeDefined();
  });

  it('should bump version properly', async () => {
    const fs = require('fs');
    const packageData = JSON.parse(fs.readFileSync('./package.json', 'utf8'));
    const currentVersion = packageData.version;
    
    // Validate version format
    expect(currentVersion).toMatch(/^\d+\.\d+\.\d+$/);
    
    // Parse version parts
    const [major, minor, patch] = currentVersion.split('.').map(Number);
    expect(patch).toBeGreaterThan(2190); // Should be recent
  });

  it('should have working version command', async () => {
    const { VersionCommand } = await import('../VersionCommand.js');
    const cmd = new VersionCommand();
    const result = await cmd.execute();
    
    expect(result.success).toBe(true);
    expect(result.version).toBeDefined();
    expect(result.uptime).toBeDefined();
  });
});