#!/usr/bin/env npx tsx
/**
 * Response Routing Fix - Ensure server responses get sent back to external WebSocket clients
 * 
 * ISSUE: Server resolves correlations internally but doesn't send responses back via WebSocket
 * FIX: After resolving correlation, route response message back through WebSocket transport
 */

import fs from 'fs';

class ResponseRoutingFix {
  
  async diagnoseIssue(): Promise<void> {
    console.log('🔍 DIAGNOSING RESPONSE ROUTING ISSUE');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    
    console.log('📊 CURRENT FLOW (from logs):');
    console.log('  1. ✅ External client sends request via WebSocket');
    console.log('  2. ✅ Server registers external correlation');
    console.log('  3. ✅ Server processes request (lists 20 commands)');
    console.log('  4. ✅ Server creates response message');
    console.log('  5. ✅ Server resolves correlation internally');
    console.log('  6. ❌ Response never gets sent back via WebSocket');
    console.log('  7. ❌ External client times out waiting for response');
    console.log('');
    
    console.log('🎯 ROOT CAUSE ANALYSIS:');
    console.log('  The server ResponseCorrelator.resolveRequest() only resolves the internal promise');
    console.log('  It does NOT automatically send the response back through WebSocket transport');
    console.log('  External clients need the response message sent via WebSocket');
    console.log('');
    
    console.log('💡 SOLUTION NEEDED:');
    console.log('  When server resolves external correlation, it should also:');
    console.log('  - Send response message back through WebSocket transport');
    console.log('  - Route to the specific WebSocket connection that made the request');
    console.log('  - Maintain correlation ID for external client to match response');
    console.log('');
    
    console.log('🔧 IMPLEMENTATION OPTIONS:');
    console.log('  Option 1: Modify ResponseCorrelator.resolveRequest() to send WebSocket responses');
    console.log('  Option 2: Add response routing after correlation resolution in JTAGRouter');
    console.log('  Option 3: Enhance WebSocket transport to handle response routing');
    console.log('');
    
    console.log('📋 RECOMMENDED APPROACH:');
    console.log('  Modify JTAGRouter response handling to send external responses via WebSocket');
    console.log('  This keeps the ResponseCorrelator focused and adds routing logic to router');
  }

  async implementFix(): Promise<void> {
    console.log('🔧 IMPLEMENTING RESPONSE ROUTING FIX');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    
    const routerPath = 'system/core/router/shared/JTAGRouter.ts';
    
    if (!fs.existsSync(routerPath)) {
      console.log(`❌ Router file not found: ${routerPath}`);
      return;
    }
    
    console.log('🔧 Adding external response routing to JTAGRouter...');
    
    let content = fs.readFileSync(routerPath, 'utf8');
    
    // Find where external correlation is resolved and add WebSocket response routing
    const insertAfter = 'const resolved = this.responseCorrelator.resolveRequest(message.correlationId, message.payload);';
    const insertIndex = content.indexOf(insertAfter);
    
    if (insertIndex === -1) {
      console.log('❌ Could not find correlation resolution point in JTAGRouter');
      return;
    }
    
    const responseRoutingCode = `
    
    // RESPONSE ROUTING FIX: Send external responses back via WebSocket
    if (resolved && message.correlationId?.startsWith('client_')) {
      console.log(\`📡 \${this.toString()}: Routing external response \${message.correlationId} back via WebSocket\`);
      
      // Find the WebSocket transport to send response back to external client
      const webSocketTransport = this.transports.get('cross-context');
      if (webSocketTransport) {
        try {
          // Send the response message back through WebSocket
          await webSocketTransport.send(message);
          console.log(\`✅ \${this.toString()}: External response sent via WebSocket for \${message.correlationId}\`);
        } catch (error) {
          console.error(\`❌ \${this.toString()}: Failed to send external response \${message.correlationId}:\`, error);
        }
      } else {
        console.warn(\`⚠️ \${this.toString()}: No WebSocket transport available for external response \${message.correlationId}\`);
      }
    }`;
    
    // Find the end of the current line to insert after
    const lineEnd = content.indexOf('\\n', insertIndex + insertAfter.length);
    const insertPoint = lineEnd + 1;
    
    // Insert the response routing code
    content = content.slice(0, insertPoint) + responseRoutingCode + content.slice(insertPoint);
    
    // Write the fixed file
    fs.writeFileSync(routerPath, content);
    console.log('✅ Response routing fix applied to JTAGRouter');
    
    console.log('');
    console.log('🎯 NEXT STEPS:');
    console.log('  1. Rebuild the system: npm run build');
    console.log('  2. Restart the system: npm run system:restart');
    console.log('  3. Test screenshot: npx tsx scripts/screenshot.ts');
  }
}

// Run the diagnosis and fix
const fix = new ResponseRoutingFix();
fix.diagnoseIssue().then(() => fix.implementFix()).catch(console.error);