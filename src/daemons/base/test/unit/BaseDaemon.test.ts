/**
 * BaseDaemon Unit Tests
 * Tests the base daemon functionality that all other daemons inherit
 */

/* eslint-env jest */
import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { BaseDaemon } from '../../BaseDaemon';
import { DaemonType } from '../../DaemonTypes';
import type { DaemonMessage, DaemonResponse } from '../../DaemonProtocol';

class TestDaemon extends BaseDaemon {
  public readonly name = 'test-daemon';
  public readonly version = '1.0.0';
  public readonly daemonType = DaemonType.COMMAND_PROCESSOR;

  protected async onStart(): Promise<void> {
    this.log('Test daemon started');
  }

  protected async onStop(): Promise<void> {
    this.log('Test daemon stopped');
  }

  async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    return {
      success: true,
      data: { echo: message.data }
    };
  }
}

describe('BaseDaemon', () => {
  let daemon: TestDaemon;

  beforeEach(() => {
    daemon = new TestDaemon();
  });

  afterEach(async () => {
    if (daemon.isRunning) {
      await daemon.stop();
    }
  });

  it('should initialize with correct properties', () => {
    expect(daemon.name).toBe('test-daemon');
    expect(daemon.version).toBe('1.0.0');
    expect(daemon.daemonType).toBe(DaemonType.COMMAND_PROCESSOR);
    expect(daemon.isRunning).toBe(false);
  });

  it('should start and stop correctly', async () => {
    expect(daemon.isRunning).toBe(false);
    
    await daemon.start();
    expect(daemon.isRunning).toBe(true);
    
    await daemon.stop();
    expect(daemon.isRunning).toBe(false);
  });

  it('should handle messages when running', async () => {
    await daemon.start();
    
    const testMessage: DaemonMessage = {
      id: 'test-123',
      from: 'test-sender',
      to: 'test-daemon',
      type: 'test-message',
      data: { hello: 'world' },
      timestamp: new Date()
    };

    const response = await daemon.handleMessage(testMessage);
    
    expect(response.success).toBe(true);
    expect(response.data).toEqual({ echo: { hello: 'world' } });
  });

  it('should log messages with proper format', () => {
    const logSpy = jest.spyOn(console, 'log').mockImplementation();
    
    daemon.log('Test message');
    
    expect(logSpy).toHaveBeenCalledWith(
      expect.stringContaining('[test-daemon]'),
      expect.stringContaining('Test message')
    );
    
    logSpy.mockRestore();
  });
});