#!/usr/bin/env npx tsx
/**
 * Debug WebSocket Response Correlation Issue
 * 
 * This script creates an extremely verbose client to trace exactly where
 * the WebSocket response correlation is failing.
 */

import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';

class CorrelationDebugger {
  
  async testCorrelation() {
    console.log(`🐛 === CORRELATION DEBUG SESSION START ===`);
    
    try {
      console.log(`🔌 Step 1: Connecting to JTAG system...`);
      const connectResult = await JTAGClientServer.connect({
        targetEnvironment: 'server'
      });
      
      const client = connectResult.client;
      console.log(`✅ Step 1: Connected successfully`);
      
      console.log(`🔍 Step 2: Testing command discovery with detailed tracing...`);
      
      // Intercept all transport messages to see what's actually happening
      const originalHandler = client.handleTransportMessage.bind(client);
      client.handleTransportMessage = async (message) => {
        console.log(`🔔 === TRANSPORT MESSAGE RECEIVED ===`);
        console.log(`📨 Message Type: ${message.messageType}`);
        console.log(`📨 Correlation ID: ${message.correlationId}`);
        console.log(`📨 Endpoint: ${message.endpoint}`);
        console.log(`📨 Full Message:`, JSON.stringify(message, null, 2));
        console.log(`🔔 === END MESSAGE ===`);
        
        return await originalHandler(message);
      };
      
      console.log(`📋 Step 3: Attempting command discovery...`);
      const commands = await client.listCommands();
      console.log(`✅ Step 3: Commands discovered:`, commands);
      
    } catch (error) {
      console.error(`❌ Correlation debug failed:`, error);
      
      // Show detailed error analysis 
      if (error.message.includes('timeout')) {
        console.log(`🔍 TIMEOUT ANALYSIS:`);
        console.log(`- This means the request was sent but no response arrived`);
        console.log(`- Check server logs to see if message was processed`);
        console.log(`- Verify transport configuration matches between client/server`);
      }
    }
    
    console.log(`🐛 === CORRELATION DEBUG SESSION END ===`);
  }
}

async function main() {
  const correlationDebugger = new CorrelationDebugger();
  await correlationDebugger.testCorrelation();
}

main().catch(console.error);