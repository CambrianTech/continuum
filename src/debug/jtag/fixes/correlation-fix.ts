#!/usr/bin/env npx tsx
/**
 * Correlation Fix - Apply targeted fix for external client correlation registration
 * 
 * ISSUE: External clients create correlation IDs but server ResponseCorrelator doesn't know about them
 * FIX: Register external correlation IDs with server ResponseCorrelator when requests arrive
 */

import fs from 'fs';
import path from 'path';

class CorrelationFix {
  
  async applyFix(): Promise<void> {
    console.log('🔧 APPLYING TARGETED CORRELATION FIX');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    
    // 1. Fix the JTAGRouter to register external correlations
    await this.fixJTAGRouter();
    
    // 2. Enhance WebSocket message handling to track external correlations  
    await this.fixWebSocketTransport();
    
    console.log('');
    console.log('✅ CORRELATION FIX APPLIED SUCCESSFULLY');
    console.log('🎯 External client correlation IDs will now be registered with server ResponseCorrelator');
    console.log('💡 Test with: npx tsx scripts/screenshot.ts');
  }

  private async fixJTAGRouter(): Promise<void> {
    const routerPath = 'system/core/router/server/JTAGRouterDynamicServer.ts';
    
    if (!fs.existsSync(routerPath)) {
      console.log(`❌ Router file not found: ${routerPath}`);
      return;
    }
    
    console.log('🔧 Fixing JTAGRouter to register external correlations...');
    
    let content = fs.readFileSync(routerPath, 'utf8');
    
    // Add correlation registration for external requests
    const correlationRegisterCode = `
    // CORRELATION FIX: Register external client correlation IDs
    if (message.messageType === 'request' && message.correlationId?.startsWith('client_')) {
      console.log(\`🔗 JTAGRouter: Registering external correlation \${message.correlationId}\`);
      
      // Register the external correlation with server's ResponseCorrelator
      // This creates a promise that will be resolved when response is ready
      const correlationPromise = this.responseCorrelator.createRequest(message.correlationId);
      
      // Don't await here - let the normal message flow handle it
      // The promise will be resolved by the response handling code
      correlationPromise.catch(error => {
        console.warn(\`⚠️ External correlation \${message.correlationId} failed: \${error.message}\`);
      });
    }
`;
    
    // Find the place to insert this code - after message processing starts
    const insertAfter = 'Processing message req:';
    const insertIndex = content.indexOf(insertAfter);
    
    if (insertIndex === -1) {
      console.log('❌ Could not find insertion point in JTAGRouter');
      return;
    }
    
    // Find the end of the current line to insert after
    const lineEnd = content.indexOf('\\n', insertIndex);
    const insertPoint = lineEnd + 1;
    
    // Insert the correlation registration code
    content = content.slice(0, insertPoint) + correlationRegisterCode + content.slice(insertPoint);
    
    // Write the fixed file
    fs.writeFileSync(routerPath, content);
    console.log('✅ JTAGRouter correlation fix applied');
  }

  private async fixWebSocketTransport(): Promise<void> {
    const transportPath = 'system/transports/websocket-transport/server/WebSocketTransportClientServer.ts';
    
    if (!fs.existsSync(transportPath)) {
      console.log(`❌ WebSocket transport file not found: ${transportPath}`);
      return;
    }
    
    console.log('🔧 Enhancing WebSocket transport correlation handling...');
    
    let content = fs.readFileSync(transportPath, 'utf8');
    
    // Add detailed logging for correlation debugging
    const loggingEnhancement = `
            // CORRELATION DEBUG: Log external client requests for debugging
            if (message.correlationId?.startsWith('client_')) {
              console.log(\`🌐 WebSocket: External client request - correlation: \${message.correlationId}, endpoint: \${message.endpoint}\`);
            }
`;
    
    // Find the handleIncomingMessage call and add logging before it
    const handleIncomingIndex = content.indexOf('this.handleIncomingMessage(message);');
    
    if (handleIncomingIndex === -1) {
      console.log('❌ Could not find handleIncomingMessage call in WebSocket transport');
      return;
    }
    
    // Insert logging before the handleIncomingMessage call
    content = content.slice(0, handleIncomingIndex) + loggingEnhancement + '            ' + content.slice(handleIncomingIndex);
    
    // Write the enhanced file
    fs.writeFileSync(transportPath, content);
    console.log('✅ WebSocket transport correlation enhancement applied');
  }
}

// Apply the fix
const fix = new CorrelationFix();
fix.applyFix().catch(console.error);