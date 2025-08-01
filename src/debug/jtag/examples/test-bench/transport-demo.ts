#!/usr/bin/env npx tsx
/**
 * JTAG Transport Abstraction Demo
 * 
 * This demonstrates how JTAG's transport system allows you to:
 * 1. Add new transport backends (REST API, MCP, etc.)
 * 2. Route messages through multiple transports simultaneously
 * 3. Test transport-agnostic screenshot and logging
 * 
 * The beauty of JTAG is that screenshot() and log() calls work the same
 * regardless of the transport layer - WebSocket, REST API, MCP, etc.
 */

import { JTAGBase } from '../../system/core/shared/JTAGBase';
import { jtagRouter } from '../../system/core/router/shared/JTAGRouter';
import { RestApiTransport } from '../transports/RestApiTransport';
import { JTAGMessageFactory, JTAG_MESSAGE_TYPES, JTAG_CONTEXTS } from '../../system/core/types/JTAGTypes';

async function demonstrateTransportAbstraction() {
  console.log('🎪 JTAG Transport Abstraction Demo\n');

  // 1. Initialize JTAG system
  console.log('📋 Step 1: Initialize JTAG system');
  JTAGBase.initialize({
    context: 'server',
    enableConsoleOutput: true,
    jtagPort: 9001
  });

  // 2. Register new REST API transport
  console.log('\n📋 Step 2: Register REST API transport');
  const restTransport = new RestApiTransport('http://localhost:8080');
  jtagRouter.registerTransport(restTransport);
  
  // Register REST-specific routes
  RestApiTransport.registerRoutes(jtagRouter);

  // 3. Test logging through multiple transports
  console.log('\n📋 Step 3: Test multi-transport logging');
  
  // This log goes through default routes (file system + WebSocket)
  JTAGBase.log('TRANSPORT_DEMO', 'Standard log message - routes to files & WebSocket');
  
  // This log goes through REST API (because of conditional routing)
  const restLogMessage = JTAGMessageFactory.createRequest(
    JTAG_MESSAGE_TYPES.LOG,
    JTAG_CONTEXTS.SERVER,
    {
      level: 'log',
      message: 'REST API log message - routes to HTTP endpoint',
      component: 'TRANSPORT_DEMO',
      restApiTarget: true  // Triggers REST API routing
    }
  );
  
  console.log('\n🚀 Routing message through JTAG router...');
  const logResults = await jtagRouter.routeMessage(restLogMessage);
  console.log('📊 Log routing results:', logResults.map(r => `${r.transport}: ${r.success ? '✅' : '❌'}`).join(', '));

  // 4. Test screenshots through multiple transports  
  console.log('\n📋 Step 4: Test multi-transport screenshots');
  
  // Standard screenshot (goes to files + WebSocket)
  console.log('📸 Taking standard screenshot...');
  const standardScreenshot = await JTAGBase.screenshot('transport-demo-standard', {
    width: 800,
    height: 600
  });
  console.log('📊 Standard screenshot result:', standardScreenshot.success ? '✅' : '❌');

  // API screenshot (goes to REST API)
  console.log('📸 Taking API screenshot...');
  const apiScreenshotMessage = JTAGMessageFactory.createRequest(
    JTAG_MESSAGE_TYPES.SCREENSHOT,
    JTAG_CONTEXTS.SERVER,
    {
      filename: 'transport-demo-api',
      format: 'api',  // Triggers REST API routing
      width: 800,
      height: 600,
      metadata: { transport: 'rest-api-demo' }
    }
  );
  
  const screenshotResults = await jtagRouter.routeMessage(apiScreenshotMessage);
  console.log('📊 API screenshot routing results:', screenshotResults.map(r => `${r.transport}: ${r.success ? '✅' : '❌'}`).join(', '));

  // 5. Show route table 
  console.log('\n📋 Step 5: Current route table');
  const routeTable = jtagRouter.getRouteTable();
  console.log('🗺️  Active routes:');
  for (const [routeId, routes] of Array.from(routeTable.routes.entries())) {
    for (const route of routes) {
      if (route.enabled) {
        console.log(`   • ${routeId} → ${route.transport} (priority: ${route.metadata?.priority})`);
      }
    }
  }

  // 6. Demonstrate context-aware routing
  console.log('\n📋 Step 6: Context-aware routing demo');
  
  // Browser-context message
  const browserMessage = JTAGMessageFactory.createRequest(
    JTAG_MESSAGE_TYPES.LOG,
    JTAG_CONTEXTS.BROWSER,
    {
      level: 'log',
      message: 'This comes from browser context',
      component: 'BROWSER_DEMO'
    }
  );
  
  // External-context message  
  const externalMessage = JTAGMessageFactory.createRequest(
    JTAG_MESSAGE_TYPES.LOG,
    JTAG_CONTEXTS.EXTERNAL,
    {
      level: 'log',
      message: 'This comes from external system',
      component: 'EXTERNAL_API',
      restApiTarget: true
    }
  );

  const browserResults = await jtagRouter.routeMessage(browserMessage);
  const externalResults = await jtagRouter.routeMessage(externalMessage);
  
  console.log('🌐 Browser message routes:', browserResults.map(r => `${r.transport}: ${r.success ? '✅' : '❌'}`).join(', '));
  console.log('🔗 External message routes:', externalResults.map(r => `${r.transport}: ${r.success ? '✅' : '❌'}`).join(', '));

  console.log('\n🎉 Transport abstraction demo complete!');
  console.log('\n💡 Key takeaway: The same screenshot() and log() API works across');
  console.log('   all transport layers - WebSocket, REST API, MCP, etc.');
  console.log('   Messages are routed intelligently based on context and metadata.\n');
}

// Run demo
demonstrateTransportAbstraction().catch(error => {
  console.error('❌ Demo failed:', error);
  process.exit(1);
});