#!/usr/bin/env npx tsx
/**
 * Raw WebSocket Debug - Test list command response format directly
 */

import WebSocket from 'ws';
import { v4 as uuidv4 } from 'uuid';

async function testRawWebSocket() {
  console.log('🔍 RAW WEBSOCKET LIST COMMAND TEST');
  console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
  
  return new Promise((resolve, reject) => {
    const ws = new WebSocket('ws://localhost:9001');
    const correlationId = `client_${Date.now()}_${uuidv4().slice(0, 8)}`;
    
    let responseReceived = false;
    
    ws.on('open', () => {
      console.log('✅ WebSocket connected to server');
      
      // Send list command request
      const message = {
        messageType: 'request',
        correlationId,
        timestamp: new Date().toISOString(),
        context: {
          environment: 'server',
          uuid: '00000000-0000-0000-0000-000000000000'
        },
        sessionId: '00000000-0000-0000-0000-000000000000',
        endpoint: 'server/commands/list',
        origin: 'client',
        payload: {
          context: {
            environment: 'server',
            uuid: '00000000-0000-0000-0000-000000000000'
          },
          sessionId: '00000000-0000-0000-0000-000000000000',
          category: 'all',
          includeDescription: true,
          includeSignature: true
        }
      };
      
      console.log('📤 Sending list request:', JSON.stringify(message, null, 2));
      ws.send(JSON.stringify(message));
    });
    
    ws.on('message', (data) => {
      if (responseReceived) return; // Only process first response
      responseReceived = true;
      
      try {
        const response = JSON.parse(data.toString());
        console.log('📥 RAW RESPONSE RECEIVED:');
        console.log('Type:', typeof response);
        console.log('Keys:', Object.keys(response));
        console.log('Correlation ID:', response.correlationId);
        console.log('Endpoint:', response.endpoint);
        console.log('Origin:', response.origin);
        
        if (response.payload) {
          console.log('\n🔍 PAYLOAD ANALYSIS:');
          console.log('Payload type:', typeof response.payload);
          console.log('Payload keys:', Object.keys(response.payload));
          console.log('Payload success:', response.payload.success);
          console.log('Payload totalCount:', response.payload.totalCount);
          
          if (response.payload.commands !== undefined) {
            console.log('\n🎯 COMMANDS PROPERTY:');
            console.log('Commands type:', typeof response.payload.commands);
            console.log('Commands constructor:', response.payload.commands?.constructor?.name);
            console.log('Commands is array:', Array.isArray(response.payload.commands));
            console.log('Commands length:', response.payload.commands?.length);
            
            if (response.payload.commands && typeof response.payload.commands[Symbol.iterator] === 'function') {
              console.log('✅ Commands IS iterable');
            } else {
              console.log('❌ Commands is NOT iterable');
            }
            
            console.log('Commands value (first 500 chars):', JSON.stringify(response.payload.commands, null, 2).slice(0, 500));
          } else {
            console.log('❌ No commands property found in payload');
          }
        }
        
        console.log('\n📋 FULL RESPONSE (first 1000 chars):');
        console.log(JSON.stringify(response, null, 2).slice(0, 1000));
        
        ws.close();
        resolve(response);
        
      } catch (error) {
        console.error('❌ Failed to parse response:', error);
        console.log('Raw data:', data.toString());
        ws.close();
        reject(error);
      }
    });
    
    ws.on('error', (error) => {
      console.error('❌ WebSocket error:', error);
      reject(error);
    });
    
    ws.on('close', () => {
      console.log('🔌 WebSocket connection closed');
      if (!responseReceived) {
        reject(new Error('Connection closed without receiving response'));
      }
    });
    
    // Timeout after 10 seconds
    setTimeout(() => {
      if (!responseReceived) {
        console.log('⏰ Request timed out');
        ws.close();
        reject(new Error('Request timed out'));
      }
    }, 10000);
  });
}

testRawWebSocket()
  .then(() => console.log('✅ Test completed successfully'))
  .catch(error => console.error('❌ Test failed:', error));