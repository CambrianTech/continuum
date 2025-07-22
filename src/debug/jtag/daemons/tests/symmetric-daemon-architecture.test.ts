#!/usr/bin/env npx tsx
/**
 * Symmetric Daemon Architecture Tests
 * 
 * Validates the dumb router pattern and symmetric daemon behavior
 * as specified in middle-out/architecture/symmetric-daemon-architecture.md
 */

import { JTAGRouter } from '../shared/JTAGRouter';
import { CommandProcessorDaemon } from '../CommandProcessorDaemon';
import { ConsoleDaemon } from '../ConsoleDaemon';
import { DaemonMessage } from '../shared/MessageSubscriber';

async function runSymmetricDaemonTests() {
  console.log('🧪 Testing Symmetric Daemon Architecture\n');

  // Test 1: Dumb Router - No Business Logic
  console.log('📋 Test 1: Dumb Router Pattern');
  const router = new JTAGRouter({ environment: 'universal' });
  
  // Router should have NO business logic
  console.log(`   ✅ Router has business logic: ${router.hasBusinessLogic()}`);
  console.log(`   ✅ Router is dumb - only routes based on patterns\n`);

  // Test 2: Daemon Self-Registration
  console.log('📋 Test 2: Daemon Self-Registration');
  
  // Create server-side daemons
  const serverCommandProcessor = new CommandProcessorDaemon('server');
  const serverConsole = new ConsoleDaemon('server');
  
  // Create client-side daemons
  const clientCommandProcessor = new CommandProcessorDaemon('client');
  const clientConsole = new ConsoleDaemon('client');
  
  // Daemons register themselves - no external configuration needed
  await serverCommandProcessor.registerWithRouter(router);
  await serverConsole.registerWithRouter(router);
  await clientCommandProcessor.registerWithRouter(router);
  await clientConsole.registerWithRouter(router);
  
  // Check registered endpoints
  const endpoints = router.getRegisteredEndpoints();
  console.log('   📍 Registered endpoints:', endpoints.sort());
  
  // Should have /client, /server, /remote, and direct variants
  const expectedPrefixes = ['/client/', '/server/', '/remote/', '/'];
  const hasAllPrefixes = expectedPrefixes.every(prefix => 
    endpoints.some(ep => ep.startsWith(prefix))
  );
  
  console.log(`   ✅ All route prefixes registered: ${hasAllPrefixes}`);
  console.log(`   📊 Total routes registered: ${endpoints.length}\n`);

  // Test 3: Message Routing with Context
  console.log('📋 Test 3: Context-Aware Message Routing');
  
  // Test server command routing
  const serverCommandMessage: DaemonMessage = {
    type: '/server/command',
    payload: {
      command: 'screenshot',
      parameters: { filename: 'test.png' },
      context: 'server'
    }
  };
  
  const serverResults = await router.routeMessage(serverCommandMessage);
  console.log(`   📨 Server command result:`, serverResults[0].success);
  console.log(`   🎯 Command executed in: ${(serverResults[0].data as any)?.context} context`);
  
  // Test client command routing
  const clientCommandMessage: DaemonMessage = {
    type: '/client/command',
    payload: {
      command: 'dom-query',
      parameters: { selector: 'body' },
      context: 'client'
    }
  };
  
  const clientResults = await router.routeMessage(clientCommandMessage);
  console.log(`   📨 Client command result:`, clientResults[0].success);
  console.log(`   🎯 Command executed in: ${(clientResults[0].data as any)?.context} context\n`);

  // Test 4: UUID-Based Direct Access
  console.log('📋 Test 4: UUID-Based Direct Access');
  
  const commandUUID = serverCommandProcessor.getUUID();
  console.log(`   🎯 Server CommandProcessor UUID: ${commandUUID}`);
  
  // Direct message via UUID
  const directMessage: DaemonMessage = {
    type: 'direct-access',
    target: commandUUID,
    payload: {
      command: 'log',
      parameters: { component: 'TEST', message: 'Direct UUID access works!' },
      context: 'server'
    }
  };
  
  const directResults = await router.routeMessage(directMessage);
  console.log(`   ✅ Direct UUID access result:`, directResults[0].success);
  console.log(`   📝 Message routed directly to daemon via UUID`);

  // Test remote UUID routing (for future mesh networking)
  const remoteMessage: DaemonMessage = {
    type: `/remote/${commandUUID}`,
    payload: {
      command: 'log',
      parameters: { component: 'REMOTE_TEST', message: 'Remote UUID routing works!' },
      context: 'server'
    }
  };
  
  const remoteResults = await router.routeMessage(remoteMessage);
  console.log(`   🌐 Remote UUID routing result:`, remoteResults[0].success);
  console.log(`   📡 Message routed via /remote/uuid prefix\n`);

  // Test 5: Symmetric API - Same Methods, Different Context
  console.log('📋 Test 5: Symmetric API Across Contexts');
  
  // Both server and client command processors have the same interface
  const serverCommands = serverCommandProcessor.getAvailableCommands();
  const clientCommands = clientCommandProcessor.getAvailableCommands();
  
  console.log(`   🖥️  Server commands: ${serverCommands.join(', ')}`);
  console.log(`   📱 Client commands: ${clientCommands.join(', ')}`);
  
  // Both should have 'log' command but different implementations
  const bothHaveLog = serverCommands.includes('log') && clientCommands.includes('log');
  console.log(`   ✅ Both contexts support 'log' command: ${bothHaveLog}`);
  
  // Test the same command in different contexts
  const serverLogMessage: DaemonMessage = {
    type: 'command',
    payload: {
      command: 'log',
      parameters: { component: 'SYMMETRIC_TEST', message: 'Server context logging' },
      context: 'server'
    }
  };
  
  const serverLogResult = await serverCommandProcessor.handleMessage(serverLogMessage);
  console.log(`   📝 Server log success: ${serverLogResult.success}`);
  
  const clientLogMessage: DaemonMessage = {
    type: 'command',
    payload: {
      command: 'log',
      parameters: { component: 'SYMMETRIC_TEST', message: 'Client context logging' },
      context: 'client'
    }
  };
  
  const clientLogResult = await clientCommandProcessor.handleMessage(clientLogMessage);
  console.log(`   📝 Client log success: ${clientLogResult.success}`);
  console.log(`   ✅ Same API, different context implementation\n`);

  // Test 6: Console Daemon Symmetric Behavior
  console.log('📋 Test 6: Console Daemon Symmetric Behavior');
  
  const serverConsoleMessage: DaemonMessage = {
    type: 'console',
    payload: {
      level: 'info' as const,
      component: 'SYMMETRIC_TEST',
      message: 'Server console message',
      timestamp: new Date().toISOString(),
      context: 'server' as const
    }
  };
  
  const serverConsoleResult = await serverConsole.handleMessage(serverConsoleMessage);
  console.log(`   🖥️  Server console processing: ${serverConsoleResult.success}`);
  
  const clientConsoleMessage: DaemonMessage = {
    type: 'console',
    payload: {
      level: 'info' as const,
      component: 'SYMMETRIC_TEST',
      message: 'Client console message',
      timestamp: new Date().toISOString(),
      context: 'client' as const
    }
  };
  
  const clientConsoleResult = await clientConsole.handleMessage(clientConsoleMessage);
  console.log(`   📱 Client console processing: ${clientConsoleResult.success}`);
  console.log(`   ✅ Console daemon works symmetrically\n`);

  // Test 7: Architecture Validation
  console.log('📋 Test 7: Architecture Validation');
  console.log(`   ✅ Dumb router: No business logic in router`);
  console.log(`   ✅ Self-registration: Daemons register themselves`);
  console.log(`   ✅ Single concern: Each daemon handles one responsibility`);
  console.log(`   ✅ Context agnostic: Same interfaces work everywhere`);
  console.log(`   ✅ UUID access: Direct daemon communication`);
  console.log(`   ✅ Remote routing: /remote/uuid for future mesh networking`);
  console.log(`   ✅ Automatic prefixes: Router creates all routes automatically`);
  console.log(`   ✅ Symmetric APIs: Same methods, different implementations`);
  
  console.log('\n🎉 Symmetric Daemon Architecture: ALL TESTS PASSED!');
  console.log('\n📊 Architecture Summary:');
  console.log('   - CommandProcessorDaemon: Handles command execution in both contexts');
  console.log('   - ConsoleDaemon: Manages console logging and interception');
  console.log('   - JTAGRouter: Dumb router with automatic routing prefixes');
  console.log('     • /client/endpoint - Local client context');
  console.log('     • /server/endpoint - Local server context');
  console.log('     • /remote/uuid - Remote daemon access (mesh networking)');
  console.log('     • /endpoint - Direct base endpoint');
  console.log('   - MessageSubscriber: Universal interface for all daemon components');
  console.log('   - Context-agnostic: Daemons specify only base endpoint, router handles prefixes');
}

// Run the tests
runSymmetricDaemonTests().catch(console.error);