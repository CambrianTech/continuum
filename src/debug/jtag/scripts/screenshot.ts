#!/usr/bin/env tsx
/**
 * Smart screenshot command - Universal screenshot with smart connection defaults
 * Tests the complete transport and command system end-to-end
 */

import { JTAGClientServer } from '../system/core/client/server/JTAGClientServer';
import { ensureJTAGSystemRunning } from './smart-system-startup';

async function takeScreenshot() {
  let jtag: any = null;
  try {
    console.log('🎯 Universal Screenshot Test - Smart Connection');
    
    // Use smart system startup
    console.log('🔄 Ensuring JTAG system is running...');
    const systemReady = await ensureJTAGSystemRunning();
    if (!systemReady) {
      throw new Error('Failed to start JTAG system');
    }
    console.log('✅ JTAG system is ready');
    
    // Connect with zero params - should auto-join existing session
    console.log('🔗 Connecting with zero params (should auto-join existing session)...');
    const { client: jtagClient, listResult } = await JTAGClientServer.connect();
    jtag = jtagClient;
    
    console.log(`🆔 Connected with session: ${jtag.sessionId}`);
    console.log(`📋 Available commands: ${listResult.totalCount}`);
    console.log('📸 Taking screenshot...');
    
    const result = await jtag.commands.screenshot({
      querySelector: 'body',
      filename: 'universal-screenshot.png'
    });
    
    console.log('✅ Screenshot taken!');
    console.log('📁 Result:', result);
    
  } catch (error) {
    console.error('❌ Screenshot failed:', error);
    console.log('💡 This tests the complete universal command system');
    console.log('💡 Error details may show what needs to be implemented');
  } finally {
    // Disconnect client to allow clean exit
    if (jtag) {
      console.log('🔌 Disconnecting client...');
      await jtag.disconnect();
      console.log('✅ Client disconnected - exiting cleanly');
    }
  }
}

takeScreenshot();