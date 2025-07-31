#!/usr/bin/env tsx
/**
 * Smart screenshot command - uses JTAGClient with DEADBEEF bootstrap for proper session management
 * Ensures system is running before attempting screenshot
 */

import { JTAGClientServer } from '../server/JTAGClientServer';
import type { JTAGClientConnectOptions } from '../shared/JTAGClient';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

/**
 * TODO: Replace with integration test pattern
 * Hard-coded remote connection to force transport usage instead of local system
 */
class RemoteOnlyJTAGClient extends JTAGClientServer {
  // Override to force remote connection - never return local system
  protected async getLocalSystem() {
    console.log('🔒 RemoteOnly: Forcing remote connection, ignoring local system');
    return null; // Always return null to force transport connection
  }
}

async function findUserSession(): Promise<string | undefined> {
  // Try to find an active user session by checking browser sessionStorage
  // This is a heuristic - a real implementation would query SessionDaemon
  console.log('🔍 Looking for active user session...');
  
  // For now, use a common user session pattern (will be replaced with SessionDaemon query)
  // Browser typically generates sessions starting with specific prefixes
  const commonUserSessions = [
    'user-session-001',
    'browser-session-001',
    'default-user-session'
  ];
  
  // Return undefined to let connect() method handle session discovery
  return undefined;
}

async function takeScreenshot() {
  try {
    console.log('🎯 Connecting to existing JTAG system via WebSocket...');
    
    // First ensure the system is running
    console.log('🔄 Ensuring JTAG system is running...');
    await execAsync('npm run system:ensure');
    console.log('✅ JTAG system is ready');
    
    // Connect as remote client to existing server (forces transport connection)
    const userSessionId = await findUserSession();
    
    const options: JTAGClientConnectOptions = {
      targetEnvironment: 'server', // We're connecting TO a server
      transportType: 'websocket',
      serverUrl: 'ws://localhost:9001',
      enableFallback: false, // Don't fallback to local - we want remote connection
      sessionId: userSessionId as any // If undefined, will use DEADBEEF UNKNOWN_SESSION
    };
    
    console.log('🔗 Connecting via WebSocket transport to running JTAG server...');
    const { client: jtag, listResult } = await RemoteOnlyJTAGClient.connect(options);
    
    console.log(`🆔 Connected with session: ${jtag.sessionId}`);
    console.log(`📋 Available commands: ${listResult.totalCount}`);
    console.log('📸 Taking screenshot...');
    
    const result = await jtag.commands.screenshot({
      querySelector: 'body',
      filename: 'widget-design.png'
    });
    
    console.log('✅ Screenshot taken!');
    console.log('📁 Result:', result);
    
  } catch (error) {
    console.error('❌ Screenshot failed:', error);
    console.log('💡 Make sure the JTAG system is running with: npm start');
    console.log('💡 And that a browser is connected to create a user session');
  }
}

takeScreenshot();