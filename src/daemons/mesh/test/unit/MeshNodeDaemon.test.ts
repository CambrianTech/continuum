/**
 * MeshNodeDaemon Unit Tests
 * Tests the mesh network daemon functionality
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { MeshNodeDaemon } from '../../MeshNodeDaemon';

describe('MeshNodeDaemon', () => {
  let daemon: MeshNodeDaemon;

  beforeEach(() => {
    daemon = new MeshNodeDaemon();
  });

  afterEach(async () => {
    if (daemon.isRunning) {
      await daemon.stop();
    }
  });

  it('should initialize with correct properties', () => {
    expect(daemon.name).toBe('mesh-node');
    expect(daemon.version).toBeDefined();
    expect(daemon.isRunning).toBe(false);
  });

  it('should start and stop correctly', async () => {
    expect(daemon.isRunning).toBe(false);
    
    await daemon.start();
    expect(daemon.isRunning).toBe(true);
    
    await daemon.stop();
    expect(daemon.isRunning).toBe(false);
  });

  it('should handle basic message types', async () => {
    await daemon.start();
    
    const testMessage = {
      id: 'test-123',
      from: 'test-sender',
      to: 'mesh-node',
      type: 'ping',
      data: {},
      timestamp: new Date()
    };

    const response = await daemon.handleMessage(testMessage);
    
    expect(response).toBeDefined();
    expect(typeof response.success).toBe('boolean');
  });
});