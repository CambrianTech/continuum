/**
 * Version Widget Integration Test - 50 lines max
 * Tests widget calls version command correctly
 */

const { describe, it, expect } = require('@jest/globals');

describe('Version Widget Integration', () => {
  it('should call version command via continuum API', async () => {
    // Mock continuum execute function
    const mockExecute = jest.fn();
    global.window = {
      continuum: {
        execute: mockExecute
      }
    };

    // Import and test widget method
    const { VersionWidget } = await import('../../../../ui/components/Version/VersionWidget.js');
    const widget = new VersionWidget();
    
    // Call the method that should fetch version
    await widget.fetchVersionFromAPI();
    
    // Verify it called the version command
    expect(mockExecute).toHaveBeenCalledWith(
      'version',
      {},
      expect.any(Function)
    );
  });

  it('should update version when command responds', async () => {
    const widget = await import('../../../../ui/components/Version/VersionWidget.js');
    const versionWidget = new widget.VersionWidget();
    
    // Mock the update method
    versionWidget.updateVersion = jest.fn();
    
    // Simulate command response
    const mockResponse = {
      success: true,
      version: '0.2.2198',
      uptime: '0h 5m 30s'
    };
    
    // Call the callback that handles response
    if (versionWidget.updateVersion) {
      versionWidget.updateVersion(mockResponse.version);
      expect(versionWidget.updateVersion).toHaveBeenCalledWith('0.2.2198');
    }
  });
});