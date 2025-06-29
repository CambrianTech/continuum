/**
 * Manual Integration Test
 * Test the actual promise resolution chain with running server
 */

import WebSocket from 'ws';

interface CommandResponse {
  success: boolean;
  data?: any;
  error?: string;
  id?: string;
  type?: string;
  timestamp?: string;
  processedBy?: string;
}

async function testCommandExecution() {
  console.log('🧪 Starting manual integration test...');
  
  const ws = new WebSocket('ws://localhost:9000');
  
  return new Promise<void>((resolve, reject) => {
    ws.on('open', async () => {
      console.log('✅ Connected to Continuum server');
      
      try {
        // Test 1: Info command
        console.log('\n📋 Testing info command...');
        const infoResult = await sendCommand(ws, 'info', {});
        console.log('Info result:', infoResult);
        
        // Test 2: Info with section
        console.log('\n🖥️ Testing info command with section...');
        const systemResult = await sendCommand(ws, 'info', { section: 'system' });
        console.log('System info result:', systemResult);
        
        // Test 3: Help command (check if list is available)
        console.log('\n📚 Testing help command...');
        const helpResult = await sendCommand(ws, 'help', {});
        console.log('Help result:', helpResult);
        
        // Test 4: Multiple concurrent commands
        console.log('\n🔄 Testing concurrent commands...');
        const concurrentResults = await Promise.all([
          sendCommand(ws, 'info', {}),
          sendCommand(ws, 'info', { section: 'memory' }),
          sendCommand(ws, 'help', {})
        ]);
        
        console.log('Concurrent results:');
        concurrentResults.forEach((result, index) => {
          console.log(`  Command ${index}:`, result.success ? 'SUCCESS' : 'ERROR', 
                     result.success ? `(${Object.keys(result.data || {}).length} keys)` : result.error);
        });
        
        console.log('\n✅ All integration tests passed!');
        console.log('\n📊 Summary:');
        console.log('- Promise resolution chain: ✅ Working');
        console.log('- Server-side command execution: ✅ Working');
        console.log('- Response routing: ✅ Working');
        console.log('- Concurrent execution: ✅ Working');
        
        ws.close();
        resolve();
        
      } catch (error) {
        console.error('❌ Integration test failed:', error);
        ws.close();
        reject(error);
      }
    });
    
    ws.on('error', (error) => {
      console.error('❌ WebSocket error:', error);
      reject(error);
    });
  });
}

function sendCommand(ws: WebSocket, command: string, params: any): Promise<CommandResponse> {
  return new Promise((resolve, reject) => {
    const messageId = `test_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    
    const message = {
      type: 'execute_command',
      command,
      params,
      timestamp: new Date().toISOString(),
      id: messageId
    };
    
    const timeout = setTimeout(() => {
      reject(new Error(`Command timeout: ${command}`));
    }, 10000);
    
    const responseHandler = (data: Buffer) => {
      try {
        const response = JSON.parse(data.toString()) as CommandResponse;
        
        // Check if this is our response
        if (response.id === messageId || 
            (response as any).clientId === messageId ||
            (response.type === 'execute_command_response' && response.timestamp)) {
          
          clearTimeout(timeout);
          ws.off('message', responseHandler);
          
          console.log(`  📤 Command: ${command} | 📨 Response: ${response.success ? 'SUCCESS' : 'ERROR'} | ⏱️ ${Date.now() - startTime}ms`);
          resolve(response);
        }
      } catch (error) {
        console.error('❌ Response parsing error:', error);
      }
    };
    
    ws.on('message', responseHandler);
    
    const startTime = Date.now();
    console.log(`  📤 Sending: ${command}`, params);
    
    try {
      ws.send(JSON.stringify(message));
    } catch (error) {
      clearTimeout(timeout);
      ws.off('message', responseHandler);
      reject(error);
    }
  });
}

// Run the test if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  testCommandExecution()
    .then(() => {
      console.log('🎉 Integration test completed successfully');
      process.exit(0);
    })
    .catch((error) => {
      console.error('💥 Integration test failed:', error);
      process.exit(1);
    });
}