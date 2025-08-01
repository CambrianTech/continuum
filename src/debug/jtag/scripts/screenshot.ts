#!/usr/bin/env tsx
/**
 * Smart screenshot command - Universal screenshot with smart connection defaults
 * Tests the complete transport and command system end-to-end
 */

import { JTAGClientServer } from '../system/core/client/server/JTAGClientServer';
import type { JTAGClientConnectOptions } from '../system/core/client/shared/JTAGClient';
import { ensureJTAGSystemRunning } from './smart-system-startup';

async function takeScreenshot() {
  try {
    console.log('🎯 Universal Screenshot Test - Smart Connection');
    
    // Use smart system startup
    console.log('🔄 Ensuring JTAG system is running...');
    const systemReady = await ensureJTAGSystemRunning();
    if (!systemReady) {
      throw new Error('Failed to start JTAG system');
    }
    console.log('✅ JTAG system is ready');
    
    // Smart connection - let the client figure out local vs remote
    console.log('🔗 Connecting with smart defaults...');
    const { client: jtag, listResult } = await JTAGClientServer.connect({
      // Minimal options - let transport factory handle the details
      enableFallback: false // Force remote connection for testing
    });
    
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
  }
}

takeScreenshot();