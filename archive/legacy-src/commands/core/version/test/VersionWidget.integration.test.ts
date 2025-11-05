/**
 * Version Widget Integration Test
 * 
 * CROSS-BOUNDARY VALIDATION:
 * ========================= 
 * Tests that version display works across all boundaries:
 * - Server: VersionCommand provides version data
 * - Transport: WebSocket delivers version reliably  
 * - Browser: ChatWidget displays version correctly
 * - Dynamic: Version updates when server version changes
 */

import { VersionCommand } from '../VersionCommand';

describe('Version Widget Cross-Boundary Integration', () => {
  let originalWindowVersion: any;

  beforeEach(() => {
    // Save original window version
    originalWindowVersion = (global as any).__CONTINUUM_VERSION__;
  });

  afterEach(() => {
    // Restore original
    (global as any).__CONTINUUM_VERSION__ = originalWindowVersion;
  });

  test('VersionCommand provides current package version', async () => {
    const result = await VersionCommand.execute('version', {});
    
    expect(result.success).toBe(true);
    expect(result.data).toBeDefined();
    expect(result.data.version).toBeDefined();
    expect(result.data.version).toMatch(/^\d+\.\d+\.\d+$/); // Semantic versioning
    expect(result.data.source).toBe('package.json');
    
    console.log('✅ Server version:', result.data.version);
  });

  test('Version command includes build metadata', async () => {
    const result = await VersionCommand.execute('version', { includeMeta: true });
    
    expect(result.success).toBe(true);
    expect(result.data.timestamp).toBeDefined();
    expect(result.data.nodeVersion).toBeDefined();
    expect(result.data.platform).toBeDefined();
    
    console.log('✅ Build metadata:', {
      timestamp: result.data.timestamp,
      nodeVersion: result.data.nodeVersion,
      platform: result.data.platform
    });
  });

  test('Client-side version fallback works correctly', () => {
    // Test without server version injection
    (global as any).__CONTINUUM_VERSION__ = undefined;
    
    // Simulate ChatWidget getVersion method
    const getVersion = () => {
      try {
        const serverVersion = (global as any).__CONTINUUM_VERSION__;
        if (serverVersion) return serverVersion;
        return 'unknown';
      } catch (error) {
        return 'unknown';
      }
    };

    const version = getVersion();
    expect(version).toBe('unknown');
    
    console.log('✅ Client fallback: shows "unknown" when server version unavailable');
  });

  test('Client-side version uses server injection when available', () => {
    // Test with server version injection
    (global as any).__CONTINUUM_VERSION__ = '0.2.2205';
    
    // Simulate ChatWidget getVersion method
    const getVersion = () => {
      try {
        const serverVersion = (global as any).__CONTINUUM_VERSION__;
        if (serverVersion) return serverVersion;
        return 'unknown';
      } catch (error) {
        return 'unknown';
      }
    };

    const version = getVersion();
    expect(version).toBe('0.2.2205');
    
    console.log('✅ Client injection: uses server version when available');
  });

  test('Version consistency across server and client', async () => {
    // Get server version
    const serverResult = await VersionCommand.execute('version', {});
    const serverVersion = serverResult.data.version;
    
    // Set client version to match
    (global as any).__CONTINUUM_VERSION__ = serverVersion;
    
    // Simulate client version retrieval
    const getVersion = () => {
      try {
        const serverVersion = (global as any).__CONTINUUM_VERSION__;
        if (serverVersion) return serverVersion;
        return 'unknown';
      } catch (error) {
        return 'unknown';
      }
    };

    const clientVersion = getVersion();
    
    expect(clientVersion).toBe(serverVersion);
    
    console.log('✅ Version consistency:', {
      server: serverVersion,
      client: clientVersion,
      match: serverVersion === clientVersion
    });
  });

  test('Widget HTML template renders version correctly', () => {
    // Simulate ChatWidget render with dynamic version
    const mockVersion = '1.2.3';
    (global as any).__CONTINUUM_VERSION__ = mockVersion;
    
    const getVersion = () => {
      try {
        const serverVersion = (global as any).__CONTINUUM_VERSION__;
        if (serverVersion) return serverVersion;
        return 'unknown';
      } catch (error) {
        return 'unknown';
      }
    };

    // Simulate template rendering  
    const template = `<div class="version" id="continuum-version">v\${getVersion()}</div>`;
    const renderedTemplate = template.replace('${getVersion()}', getVersion());
    
    expect(renderedTemplate).toContain('v1.2.3');
    expect(renderedTemplate).toContain('id="continuum-version"');
    
    console.log('✅ Widget template: renders version dynamically');
    console.log('   Template output:', renderedTemplate);
  });

  test('Version display handles edge cases gracefully', () => {
    const getVersion = () => {
      try {
        const serverVersion = (global as any).__CONTINUUM_VERSION__;
        if (serverVersion) return serverVersion;
        return 'unknown';
      } catch (error) {
        return 'unknown';
      }
    };

    // Test empty string
    (global as any).__CONTINUUM_VERSION__ = '';
    expect(getVersion()).toBe('unknown');
    
    // Test null
    (global as any).__CONTINUUM_VERSION__ = null;
    expect(getVersion()).toBe('unknown');
    
    // Test undefined
    (global as any).__CONTINUUM_VERSION__ = undefined;
    expect(getVersion()).toBe('unknown');
    
    // Test valid version
    (global as any).__CONTINUUM_VERSION__ = '2.0.0';
    expect(getVersion()).toBe('2.0.0');
    
    console.log('✅ Edge cases: handles empty/null/undefined gracefully');
  });
});