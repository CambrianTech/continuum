/**
 * Session Context Flow Test - Isolate where sessionId gets lost
 * 
 * Process of elimination test to find exactly where the sessionId 
 * stops flowing through the system using server logs as probes
 */

import WebSocket from 'ws';

class SessionContextTracer {
  async runDiagnostic(): Promise<void> {
    console.log('🔍 Session Context Flow Diagnostic - Process of Elimination');
    console.log('=' .repeat(70));

    // Test 1: Direct HTTP API call with explicit sessionId
    await this.testHttpApiWithSessionId();
    
    // Test 2: WebSocket call with explicit sessionId  
    await this.testWebSocketWithSessionId();
    
    // Test 3: Check what context structure actually reaches ConsoleCommand
    await this.testContextStructure();
  }

  private async testHttpApiWithSessionId(): Promise<void> {
    console.log('\n📋 Test 1: HTTP API with explicit sessionId');
    console.log('-'.repeat(50));

    try {
      const response = await fetch('http://localhost:9000/api/commands/console', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          action: 'log',
          message: 'TEST_HTTP_SESSION_CONTEXT_TRACE',
          source: 'session-context-test',
          sessionId: 'development-shared-mcrl67rr-k0ohe'
        })
      });

      const result = await response.json();
      
      console.log('📤 Sent sessionId: development-shared-mcrl67rr-k0ohe');
      console.log('📥 Received sessionLogged:', result.sessionLogged);
      console.log('📥 Warning:', result.warning || 'none');
      
      if (result.sessionLogged === false) {
        console.log('❌ HTTP API: sessionId not reaching ConsoleCommand properly');
        console.log('🔍 Check server logs for: TEST_HTTP_SESSION_CONTEXT_TRACE');
      } else {
        console.log('✅ HTTP API: sessionId reaching ConsoleCommand correctly');
      }

    } catch (error) {
      console.error('❌ HTTP API test failed:', error);
    }
  }

  private async testWebSocketWithSessionId(): Promise<void> {
    console.log('\n📋 Test 2: WebSocket with explicit sessionId');
    console.log('-'.repeat(50));

    return new Promise((resolve) => {
      const ws = new WebSocket('ws://localhost:9000');
      const requestId = `session-test-${Date.now()}`;

      ws.on('open', () => {
        const message = {
          type: 'execute_command',
          data: {
            command: 'console',
            params: JSON.stringify({
              action: 'log',
              message: 'TEST_WEBSOCKET_SESSION_CONTEXT_TRACE',
              source: 'session-context-test'
            }),
            requestId,
            sessionId: 'development-shared-mcrl67rr-k0ohe' // Explicit sessionId
          }
        };

        console.log('📤 Sent WebSocket sessionId: development-shared-mcrl67rr-k0ohe');
        ws.send(JSON.stringify(message));
      });

      ws.on('message', (data) => {
        try {
          const response = JSON.parse(data.toString());
          
          if (response.type === 'connection_confirmed') {
            return;
          }

          if (response.requestId === requestId) {
            console.log('📥 Received sessionLogged:', response.data?.sessionLogged);
            console.log('📥 Warning:', response.data?.warning || 'none');
            
            if (response.data?.sessionLogged === false) {
              console.log('❌ WebSocket: sessionId not reaching ConsoleCommand properly');
              console.log('🔍 Check server logs for: TEST_WEBSOCKET_SESSION_CONTEXT_TRACE');
            } else {
              console.log('✅ WebSocket: sessionId reaching ConsoleCommand correctly');
            }

            ws.close();
            resolve();
          }
        } catch (error) {
          console.error('❌ WebSocket response parse error:', error);
          ws.close();
          resolve();
        }
      });

      setTimeout(() => {
        console.log('⏰ WebSocket test timeout');
        ws.close();
        resolve();
      }, 5000);
    });
  }

  private async testContextStructure(): Promise<void> {
    console.log('\n📋 Test 3: Context Structure Analysis');
    console.log('-'.repeat(50));
    console.log('🔍 Adding debug logging to ConsoleCommand to see actual context structure...');
    console.log('📋 Next steps:');
    console.log('   1. Check server logs for TEST_HTTP_SESSION_CONTEXT_TRACE');
    console.log('   2. Check server logs for TEST_WEBSOCKET_SESSION_CONTEXT_TRACE');
    console.log('   3. Look for any context structure debug output');
    console.log('   4. Compare HTTP vs WebSocket context differences');
    console.log('');
    console.log('💡 Expected findings:');
    console.log('   - If both fail: ConsoleCommand context parsing is wrong');
    console.log('   - If HTTP works but WebSocket fails: WebSocketDaemon context passing is wrong');
    console.log('   - If both work: Session file paths/permissions issue');
  }
}

// Execute diagnostic if run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const tracer = new SessionContextTracer();
  tracer.runDiagnostic().catch(error => {
    console.error('❌ Diagnostic failed:', error);
    process.exit(1);
  });
}