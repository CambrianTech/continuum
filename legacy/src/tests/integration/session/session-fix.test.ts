#!/usr/bin/env tsx
/**
 * Test script to verify session management fix
 */

import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';

async function testSessionReuse() {
  console.log('🧪 Testing session reuse with JTAGClientServer...');
  
  // Create multiple connections and see if they reuse sessions
  console.log('\n📋 Creating first client connection...');
  const result1 = await JTAGClientServer.connect();
  console.log(`✅ Client 1 session: ${result1.client.sessionId}`);
  await result1.client.disconnect();
  
  console.log('\n📋 Creating second client connection...');
  const result2 = await JTAGClientServer.connect();
  console.log(`✅ Client 2 session: ${result2.client.sessionId}`);
  await result2.client.disconnect();
  
  console.log('\n📋 Creating third client connection...');
  const result3 = await JTAGClientServer.connect();
  console.log(`✅ Client 3 session: ${result3.client.sessionId}`);
  await result3.client.disconnect();
  
  // Check if they're the same
  const sameSession = result1.client.sessionId === result2.client.sessionId && 
                     result2.client.sessionId === result3.client.sessionId;
  
  console.log(`\n🎯 Session reuse working: ${sameSession ? '✅ YES' : '❌ NO'}`);
  console.log(`   All used session: ${result1.client.sessionId}`);
}

testSessionReuse().catch(console.error);