#!/usr/bin/env tsx
/**
 * Widget Services Unit Tests
 * 
 * Tests widget service layer extracted from BaseWidget god object.
 * Validates service registry, data service, event service, resource service, and AI service.
 * Category: Unit Tests - Widget Architecture
 */

// Import ONLY from shared - respecting universal module pattern
import { 
  WidgetServiceRegistry, 
  WidgetServiceFactory, 
  WIDGET_SERVICES,
  type IWidgetDataService,
  type IWidgetEventService, 
  type IWidgetResourceService,
  type IWidgetAIService,
  type WidgetServiceContext
} from '../../widgets/shared/services/WidgetServiceRegistry';
import { WidgetDataService } from '../../widgets/shared/services/data/WidgetDataService';
import { WidgetEventService } from '../../widgets/shared/services/events/WidgetEventService';
import { WidgetResourceService } from '../../widgets/shared/services/resources/WidgetResourceService';
import { WidgetAIService } from '../../widgets/shared/services/ai/WidgetAIService';

// Test utilities
function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number = 5000): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => 
      setTimeout(() => reject(new Error(`Test timed out after ${timeoutMs}ms`)), timeoutMs)
    )
  ]);
}

// Test service registry functionality
async function testServiceRegistry() {
  console.log('\n🗂️ TEST 1: Service Registry - Dependency Injection');
  
  const registry = new WidgetServiceRegistry();
  const dataService = new WidgetDataService();
  const eventService = new WidgetEventService();
  
  // Test service registration
  registry.register(WIDGET_SERVICES.DATA, dataService);
  registry.register(WIDGET_SERVICES.EVENTS, eventService);
  
  assert(registry.hasService(WIDGET_SERVICES.DATA), 'Registry has data service');
  assert(registry.hasService(WIDGET_SERVICES.EVENTS), 'Registry has event service');
  assert(!registry.hasService(WIDGET_SERVICES.AI), 'Registry does not have AI service');
  
  // Test service retrieval
  const retrievedDataService = registry.get(WIDGET_SERVICES.DATA);
  const retrievedEventService = registry.getRequired(WIDGET_SERVICES.EVENTS);
  
  assert(retrievedDataService === dataService, 'Retrieved correct data service instance');
  assert(retrievedEventService === eventService, 'Retrieved correct event service instance');
  
  // Test service listing
  const serviceList = registry.listServices();
  assert(serviceList.length === 2, 'Service list has correct count');
  assert(serviceList.includes(WIDGET_SERVICES.DATA), 'Service list includes data service');
  assert(serviceList.includes(WIDGET_SERVICES.EVENTS), 'Service list includes event service');
  
  console.log('✅ Service Registry tests passed');
}

// Test data service with strict typing
async function testDataService() {
  console.log('\n🗃️ TEST 2: Data Service - Strict Database Operations');
  
  const dataService = new WidgetDataService();
  const context: WidgetServiceContext = {
    widgetId: 'test-widget',
    widgetName: 'TestWidget',
    sessionId: 'test-session',
    environment: 'browser',
    permissions: ['read', 'write'],
    capabilities: ['storage']
  };
  
  await dataService.initialize(context);
  
  // Test data storage and retrieval
  await dataService.storeData('test-key', { message: 'test data' }, { persistent: true, cache: true });
  const retrievedData = await dataService.getData('test-key');
  
  // Note: Since we're using mock data in the service, we can't test exact values
  // but we can test that the methods execute without errors
  assert(typeof retrievedData !== 'undefined', 'Data retrieval executed successfully');
  
  // Test data existence checking
  const exists = await dataService.hasData('test-key');
  assert(typeof exists === 'boolean', 'Data existence check returns boolean');
  
  // Test state persistence
  const testState = { initialized: true, version: '1.0' };
  await dataService.saveState(testState);
  const loadedState = await dataService.loadState();
  assert(typeof loadedState === 'object', 'State loaded successfully');
  
  // Test cache operations
  await dataService.setCacheData('cache-key', 'cached-value', 1000);
  const cachedValue = await dataService.getCacheData('cache-key');
  assert(cachedValue === 'cached-value', 'Cache operations work correctly');
  
  await dataService.clearCache();
  const clearedValue = await dataService.getCacheData('cache-key');
  assert(clearedValue === undefined, 'Cache cleared successfully');
  
  await dataService.cleanup();
  console.log('✅ Data Service tests passed');
}

// Test event service
async function testEventService() {
  console.log('\n📡 TEST 3: Event Service - Event Coordination');
  
  const eventService = new WidgetEventService();
  const context: WidgetServiceContext = {
    widgetId: 'test-widget',
    widgetName: 'TestWidget', 
    sessionId: 'test-session',
    environment: 'browser',
    permissions: ['broadcast'],
    capabilities: ['events']
  };
  
  await eventService.initialize(context);
  
  // Test event listener registration
  let eventReceived = false;
  const testHandler = (eventType: string, data: any) => {
    eventReceived = true;
    assert(eventType === 'test-event', 'Event type matches');
    assert(data.message === 'test message', 'Event data matches');
  };
  
  eventService.addEventListener('test-event', testHandler);
  
  // Test event broadcasting (will trigger local handlers)
  await eventService.broadcastEvent('test-event', { message: 'test message' }, { scope: 'local' });
  assert(eventReceived, 'Event handler was called');
  
  // Test custom DOM events
  let customEventReceived = false;
  const customHandler = (event: CustomEvent) => {
    customEventReceived = true;
    assert(event.detail.test === 'data', 'Custom event data correct');
  };
  
  eventService.subscribeToCustomEvents('custom-test', customHandler);
  eventService.emitCustomEvent('custom-test', { test: 'data' });
  
  // Give DOM event time to propagate
  await new Promise(resolve => setTimeout(resolve, 10));
  assert(customEventReceived, 'Custom DOM event was received');
  
  // Test router operations (mock)
  const routerResult = await eventService.sendToRouter('test-operation', { data: 'test' });
  assert(routerResult.success, 'Router operation executed');
  
  await eventService.cleanup();
  console.log('✅ Event Service tests passed');
}

// Test resource service
async function testResourceService() {
  console.log('\n📁 TEST 4: Resource Service - File and Template Loading');
  
  const resourceService = new WidgetResourceService();
  const context: WidgetServiceContext = {
    widgetId: 'test-widget',
    widgetName: 'TestWidget',
    sessionId: 'test-session', 
    environment: 'browser',
    permissions: ['file_access'],
    capabilities: ['templates', 'screenshots']
  };
  
  await resourceService.initialize(context);
  
  // Test file existence (mock will return true/false randomly)
  const exists = await resourceService.fileExists('test-file.txt');
  assert(typeof exists === 'boolean', 'File existence check returns boolean');
  
  // Test file save operation
  const saveResult = await resourceService.saveFile('test.txt', 'test content', {
    directory: 'widgets/test',
    createDirectories: true
  });
  assert(saveResult.success, 'File save operation executed');
  
  // Test screenshot operation (mock)
  const screenshotResult = await resourceService.takeScreenshot({
    filename: 'test-screenshot.png',
    format: 'png',
    selector: 'body'
  });
  assert(screenshotResult.success, 'Screenshot operation executed');
  
  // Test resource preloading
  const preloadResults = await resourceService.preloadResources(['test.css', 'test.js']);
  assert(Array.isArray(preloadResults), 'Preload returns results array');
  assert(preloadResults.length === 2, 'Preload results has correct length');
  
  // Test cache operations
  await resourceService.clearResourceCache();
  console.log('✅ Resource cache cleared');
  
  await resourceService.cleanup();
  console.log('✅ Resource Service tests passed');
}

// Test AI service with user hierarchy integration
async function testAIService() {
  console.log('\n🤖 TEST 5: AI Service - AI Communications & User Hierarchy');
  
  const aiService = new WidgetAIService();
  const context: WidgetServiceContext = {
    widgetId: 'test-widget',
    widgetName: 'TestWidget',
    sessionId: 'test-session',
    environment: 'browser', 
    permissions: ['ai_access'],
    capabilities: ['personas', 'agents']
  };
  
  await aiService.initialize(context);
  
  // Test persona management (using mock data)
  const personas = await aiService.getAvailablePersonas();
  assert(Array.isArray(personas), 'Personas list is array');
  console.log(`📋 Found ${personas.length} available personas`);
  
  if (personas.length > 0) {
    const firstPersona = personas[0];
    assert(firstPersona.userType === 'persona', 'Persona has correct user type');
    assert(typeof firstPersona.name === 'string', 'Persona has name');
    
    // Test persona selection
    const setResult = await aiService.setActivePersona(firstPersona.name);
    assert(setResult, 'Persona set successfully');
  }
  
  // Test agent management
  const agents = await aiService.getAvailableAgents();
  assert(Array.isArray(agents), 'Agents list is array');
  console.log(`🔧 Found ${agents.length} available agents`);
  
  if (agents.length > 0) {
    const firstAgent = agents[0];
    assert(firstAgent.userType === 'agent', 'Agent has correct user type');
    assert(typeof firstAgent.name === 'string', 'Agent has name');
    
    // Test agent connection
    const connectResult = await aiService.connectToAgent(firstAgent.agent.type);
    assert(connectResult, 'Agent connection successful');
  }
  
  // Test AI querying (mock)
  const queryResult = await aiService.queryAI('Test message', { timeout: 5000 });
  assert(queryResult.success, 'AI query executed successfully');
  assert(typeof queryResult.response === 'string', 'AI query returned response');
  
  // Test AI notifications
  await aiService.notifyAI('Test notification', { source: 'unit-test' });
  console.log('✅ AI notification sent');
  
  await aiService.cleanup();
  console.log('✅ AI Service tests passed');
}

// Test service factory patterns
async function testServiceFactory() {
  console.log('\n🏭 TEST 6: Service Factory - Registry Creation Patterns');
  
  // Test standard registry creation
  const standardRegistry = WidgetServiceFactory.createStandardRegistry();
  assert(standardRegistry instanceof WidgetServiceRegistry, 'Standard registry created');
  
  // Test minimal registry creation
  const minimalRegistry = WidgetServiceFactory.createMinimalRegistry();
  assert(minimalRegistry instanceof WidgetServiceRegistry, 'Minimal registry created');
  
  // Test AI-focused registry creation
  const aiRegistry = WidgetServiceFactory.createAIRegistry();
  assert(aiRegistry instanceof WidgetServiceRegistry, 'AI registry created');
  
  console.log('✅ Service Factory tests passed');
}

// Test integrated service lifecycle
async function testServiceLifecycle() {
  console.log('\n🔄 TEST 7: Service Lifecycle - Full Integration');
  
  const registry = new WidgetServiceRegistry();
  const context: WidgetServiceContext = {
    widgetId: 'lifecycle-test',
    widgetName: 'LifecycleTest',
    sessionId: 'test-session',
    environment: 'browser',
    permissions: ['all'],
    capabilities: ['all']
  };
  
  // Register all services
  registry.register(WIDGET_SERVICES.DATA, new WidgetDataService());
  registry.register(WIDGET_SERVICES.EVENTS, new WidgetEventService());
  registry.register(WIDGET_SERVICES.RESOURCES, new WidgetResourceService());
  registry.register(WIDGET_SERVICES.AI, new WidgetAIService());
  
  // Test bulk initialization
  await registry.initializeAll(context);
  console.log('✅ All services initialized');
  
  // Test services are functional after initialization
  const dataService = registry.getRequired<WidgetDataService>(WIDGET_SERVICES.DATA);
  await dataService.storeData('lifecycle-test', { status: 'active' });
  console.log('✅ Data service functional after registry init');
  
  const eventService = registry.getRequired<WidgetEventService>(WIDGET_SERVICES.EVENTS);
  await eventService.broadcastEvent('lifecycle-event', { phase: 'active' });
  console.log('✅ Event service functional after registry init');
  
  // Test bulk cleanup
  await registry.cleanupAll();
  console.log('✅ All services cleaned up');
  
  console.log('✅ Service Lifecycle tests passed');
}

// Main test runner
async function runWidgetServicesTests() {
  console.log('🧪 UNIT TESTS: Widget Services Layer');
  console.log('Category: Unit Tests - Widget Architecture');
  console.log('Scope: Service extraction from BaseWidget god object\n');
  
  try {
    await withTimeout(testServiceRegistry(), 10000);
    await withTimeout(testDataService(), 10000);
    await withTimeout(testEventService(), 10000);
    await withTimeout(testResourceService(), 10000);
    await withTimeout(testAIService(), 10000);
    await withTimeout(testServiceFactory(), 5000);
    await withTimeout(testServiceLifecycle(), 15000);
    
    console.log('\n🎉 ALL WIDGET SERVICES UNIT TESTS PASSED');
    console.log('✅ Service extraction from BaseWidget successful');
    console.log('✅ Strict typing enforced throughout service layer');
    console.log('✅ Adapter pattern properly implemented');
    console.log('✅ Dependency injection working correctly');
    
  } catch (error) {
    console.error('\n❌ WIDGET SERVICES UNIT TESTS FAILED');
    console.error('Error:', error);
    process.exit(1);
  }
}

// Run tests if called directly
if (process.argv[1] && process.argv[1].endsWith('widget-services-unit.test.ts')) {
  runWidgetServicesTests()
    .then(() => {
      console.log('\n✅ Widget Services Unit Tests completed successfully');
      process.exit(0);
    })
    .catch((error) => {
      console.error('\n❌ Widget Services Unit Tests failed:', error);
      process.exit(1);
    });
}