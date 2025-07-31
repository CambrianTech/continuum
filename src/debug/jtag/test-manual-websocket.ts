#!/usr/bin/env tsx
/**
 * Manual WebSocket Test - Connect to whatever system you have running
 * 
 * This bypasses all our client code and directly connects to test basic connectivity
 */

async function testManualWebSocket() {
  try {
    // Try multiple ports that might be running
    const portsToTry = [9001, 9002, 8080, 3000];
    
    console.log('🔍 Scanning for WebSocket servers...');
    
    for (const port of portsToTry) {
      try {
        console.log(`🔌 Trying port ${port}...`);
        
        // Dynamic import of ws
        const WebSocketModule = await eval('import("ws")');
        const WSClient = WebSocketModule.default || WebSocketModule.WebSocket;
        
        const ws = new WSClient(`ws://localhost:${port}`);
        
        await new Promise((resolve, reject) => {
          const timeout = setTimeout(() => {
            ws.close();
            reject(new Error('Connection timeout'));
          }, 3000);
          
          ws.on('open', () => {
            clearTimeout(timeout);
            console.log(`✅ Connected to port ${port}!`);
            
            // Send a simple test message
            const testMessage = {
              type: 'test',
              message: 'Hello from manual client',
              timestamp: Date.now()
            };
            
            ws.send(JSON.stringify(testMessage));
            console.log('📤 Sent test message');
            
            // Listen for responses
            ws.on('message', (data) => {
              console.log('📨 Received:', data.toString());
            });
            
            // Keep connection open briefly
            setTimeout(() => {
              ws.close();
              resolve(port);
            }, 2000);
          });
          
          ws.on('error', (error) => {
            clearTimeout(timeout);
            reject(error);
          });
        });
        
        return; // Success - exit the loop
        
      } catch (error) {
        console.log(`❌ Port ${port}: ${error.message}`);
      }
    }
    
    console.log('🔍 No WebSocket servers found on common ports');
    console.log('💡 Is your JTAG system running? Try: npm start');
    
  } catch (error) {
    console.error('❌ Test failed:', error);
  }
}

testManualWebSocket();