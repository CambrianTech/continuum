#!/usr/bin/env tsx
/**
 * Smart screenshot command - connects to user session for browser interaction
 */

import { JTAGSystemServer } from '../server/JTAGSystemServer';
import type { ConnectionParams } from '../shared/JTAGSystem';

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
    console.log('🎯 Connecting to running JTAG system for screenshot...');
    
    // Try different connection strategies
    const userSessionId = await findUserSession();
    
    let jtag;
    if (userSessionId) {
      console.log(`🔗 Connecting to specific user session: ${userSessionId}`);
      const connectionParams: ConnectionParams = {
        sessionId: userSessionId as any,
        sessionCategory: 'user',
        createIfNotExists: false
      };
      jtag = await JTAGSystemServer.connect({ 
        connection: connectionParams 
      });
    } else {
      console.log('🔗 Connecting with default session discovery...');
      // Let the system handle session discovery automatically
      jtag = await JTAGSystemServer.connect();
    }
    
    console.log(`🆔 Connected with session: ${jtag.sessionId}`);
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