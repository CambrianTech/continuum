#!/usr/bin/env tsx
/**
 * Ping Command Unit Tests
 * 
 * Tests the ping command functionality including timing, environment detection,
 * and error handling across browser and server environments.
 */

import { PingBrowserCommand } from '../../commands/ping/browser/PingBrowserCommand';
import { PingServerCommand } from '../../commands/ping/server/PingServerCommand';
import { createPingParams } from '../../commands/ping/shared/PingTypes';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../daemons/command-daemon/shared/CommandBase';

console.log('🧪 Ping Command Unit Test Suite');

// Mock command daemon for testing
class MockCommandDaemon implements ICommandDaemon {
  commands = new Map();
  register() {}
  execute() { return Promise.resolve({} as any); }
  getAvailableCommands() { return []; }
  getCommand() { return undefined; }
}

function testPingCommandCreation() {
  console.log('  📝 Testing ping command creation...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const browserContext: JTAGContext = { uuid: 'test-browser', environment: 'browser' };
      const serverContext: JTAGContext = { uuid: 'test-server', environment: 'server' };
      const mockDaemon = new MockCommandDaemon();

      // Test browser command creation
      const browserCommand = new PingBrowserCommand(browserContext, 'ping', mockDaemon);
      if (browserCommand.name !== 'ping') {
        reject(new Error('Browser command name incorrect'));
        return;
      }

      // Test server command creation
      const serverCommand = new PingServerCommand(serverContext, 'ping', mockDaemon);
      if (serverCommand.name !== 'ping') {
        reject(new Error('Server command name incorrect'));
        return;
      }

      console.log('  ✅ Ping command creation works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testPingParameterCreation() {
  console.log('  📝 Testing ping parameter creation...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const context: JTAGContext = { uuid: 'test', environment: 'browser' };
      const sessionId = 'test-session';

      // Test basic parameter creation
      const basicParams = createPingParams(context, sessionId);
      if (basicParams.message !== 'ping') {
        reject(new Error('Default message incorrect'));
        return;
      }
      if (basicParams.context !== context) {
        reject(new Error('Context not preserved'));
        return;
      }
      if (basicParams.sessionId !== sessionId) {
        reject(new Error('Session ID not preserved'));
        return;
      }

      // Test custom parameter creation
      const customParams = createPingParams(context, sessionId, {
        message: 'custom ping',
        includeTiming: false,
        includeEnvironment: false
      });
      if (customParams.message !== 'custom ping') {
        reject(new Error('Custom message not set'));
        return;
      }
      if (customParams.includeTiming !== false) {
        reject(new Error('Custom timing flag not set'));
        return;
      }

      console.log('  ✅ Ping parameter creation works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testBrowserPingExecution() {
  console.log('  📝 Testing browser ping execution...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test-browser', environment: 'browser' };
    const mockDaemon = new MockCommandDaemon();
    const command = new PingBrowserCommand(context, 'ping', mockDaemon);
    
    const params = createPingParams(context, 'test-session', {
      message: 'test ping'
    });

    command.execute(params).then((result) => {
      if (!result.success) {
        reject(new Error('Ping execution should succeed'));
        return;
      }
      if (result.message !== 'test ping') {
        reject(new Error('Message not echoed correctly'));
        return;
      }
      if (!result.environment || result.environment.type !== 'browser') {
        reject(new Error('Browser environment not detected'));
        return;
      }
      if (typeof result.roundTripTime !== 'number') {
        reject(new Error('Round trip time not measured'));
        return;
      }

      console.log('  ✅ Browser ping execution works');
      resolve();
    }).catch(reject);
  });
}

function testServerPingExecution() {
  console.log('  📝 Testing server ping execution...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test-server', environment: 'server' };
    const mockDaemon = new MockCommandDaemon();
    const command = new PingServerCommand(context, 'ping', mockDaemon);
    
    const params = createPingParams(context, 'test-session', {
      message: 'server test'
    });

    command.execute(params).then((result) => {
      if (!result.success) {
        reject(new Error('Server ping execution should succeed'));
        return;
      }
      if (result.message !== 'server test') {
        reject(new Error('Server message not echoed correctly'));
        return;
      }
      if (!result.environment || result.environment.type !== 'server') {
        reject(new Error('Server environment not detected'));
        return;
      }
      if (result.environment.nodeVersion && !result.environment.nodeVersion.startsWith('v')) {
        reject(new Error('Node version format incorrect'));
        return;
      }

      console.log('  ✅ Server ping execution works');
      resolve();
    }).catch(reject);
  });
}

function testPingTiming() {
  console.log('  📝 Testing ping timing measurement...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test', environment: 'browser' };
    const mockDaemon = new MockCommandDaemon();
    const command = new PingBrowserCommand(context, 'ping', mockDaemon);
    
    const startTime = Date.now();
    const params = createPingParams(context, 'test-session');

    command.execute(params).then((result) => {
      const endTime = Date.now();
      const actualTime = endTime - startTime;

      if (typeof result.roundTripTime !== 'number') {
        reject(new Error('Round trip time not measured'));
        return;
      }
      
      // Should be reasonably close to actual time (within 100ms tolerance)
      if (Math.abs(result.roundTripTime - actualTime) > 100) {
        reject(new Error(`Timing inaccurate: expected ~${actualTime}ms, got ${result.roundTripTime}ms`));
        return;
      }

      console.log('  ✅ Ping timing measurement works');
      resolve();
    }).catch(reject);
  });
}

function testPingWithoutTiming() {
  console.log('  📝 Testing ping without timing...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test', environment: 'browser' };
    const mockDaemon = new MockCommandDaemon();
    const command = new PingBrowserCommand(context, 'ping', mockDaemon);
    
    const params = createPingParams(context, 'test-session', {
      includeTiming: false
    });

    command.execute(params).then((result) => {
      if (result.roundTripTime !== undefined) {
        reject(new Error('Round trip time should be undefined when disabled'));
        return;
      }

      console.log('  ✅ Ping without timing works');
      resolve();
    }).catch(reject);
  });
}

function testPingWithoutEnvironment() {
  console.log('  📝 Testing ping without environment info...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test', environment: 'browser' };
    const mockDaemon = new MockCommandDaemon();
    const command = new PingBrowserCommand(context, 'ping', mockDaemon);
    
    const params = createPingParams(context, 'test-session', {
      includeEnvironment: false
    });

    command.execute(params).then((result) => {
      if (result.environment !== undefined) {
        reject(new Error('Environment should be undefined when disabled'));
        return;
      }

      console.log('  ✅ Ping without environment works');
      resolve();
    }).catch(reject);
  });
}

// Run all tests
async function runAllTests() {
  try {
    await testPingCommandCreation();
    await testPingParameterCreation();
    await testBrowserPingExecution();
    await testServerPingExecution();
    await testPingTiming();
    await testPingWithoutTiming();
    await testPingWithoutEnvironment();
    
    console.log('✅ All ping command unit tests passed!');
    console.log('\n📋 TEST SUMMARY:');
    console.log('  ✅ Command creation and initialization');
    console.log('  ✅ Parameter creation and validation');
    console.log('  ✅ Browser environment execution');
    console.log('  ✅ Server environment execution');
    console.log('  ✅ Timing measurement accuracy');
    console.log('  ✅ Optional feature toggling');
    console.log('\n🎯 Ping command is ready for integration testing!');
    
    process.exit(0);
  } catch (error) {
    console.error('❌ Ping command unit test failed:', error);
    process.exit(1);
  }
}

runAllTests();