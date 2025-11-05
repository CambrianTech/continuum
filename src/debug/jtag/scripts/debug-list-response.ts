#!/usr/bin/env npx tsx
/**
 * Debug List Response - Check what the actual server response format is
 */

import { JTAGClientServer } from '@client/server/JTAGClientServer';

async function debugListResponse() {
  console.log('🔍 DEBUGGING LIST RESPONSE FORMAT');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  
  try {
    console.log('🔗 Creating client without auto-discovery...');
    
    // Create JTAGClientServer directly without connect() to avoid auto-discovery
    const client = new JTAGClientServer();
    
    // Manually initialize the connection without discovery
    console.log('🔗 Initializing connection...');
    await client.setupConnection({
      sessionId: '00000000-0000-0000-0000-000000000000',
      enableFallback: false
    });
    
    console.log('✅ Connection ready! Making raw list request...');
    
    // Get the raw response without going through discoverCommands
    const listParams = {
      context: client.context,
      sessionId: client.context.uuid,
      category: 'all' as const,
      includeDescription: true,
      includeSignature: true
    };
    
    console.log('📤 Sending list request with params:', JSON.stringify(listParams, null, 2));
    
    const rawResponse = await client.connection.executeCommand('list', listParams);
    
    console.log('📥 RAW RESPONSE:');
    console.log('Type:', typeof rawResponse);
    console.log('Constructor:', rawResponse?.constructor?.name);
    console.log('Is Array:', Array.isArray(rawResponse));
    console.log('Keys:', Object.keys(rawResponse || {}));
    console.log('Full response:', JSON.stringify(rawResponse, null, 2));
    
    if (rawResponse && typeof rawResponse === 'object') {
      console.log('\n🔍 COMMANDS PROPERTY:');
      console.log('commands type:', typeof rawResponse.commands);
      console.log('commands constructor:', rawResponse.commands?.constructor?.name);
      console.log('commands is array:', Array.isArray(rawResponse.commands));
      console.log('commands length:', rawResponse.commands?.length);
      console.log('commands value:', rawResponse.commands);
      
      if (rawResponse.commands && typeof rawResponse.commands[Symbol.iterator] === 'function') {
        console.log('✅ Commands is iterable');
      } else {
        console.log('❌ Commands is NOT iterable');
      }
    }
    
  } catch (error) {
    console.error('❌ Debug failed:', error);
  }
}

debugListResponse().catch(console.error);