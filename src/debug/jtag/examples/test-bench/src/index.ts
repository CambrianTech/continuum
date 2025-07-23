/**
 * JTAG End-to-End Demo - Client-side TypeScript with Universal JTAG
 */

console.log('🚀 JTAG Demo script loading...');

// Import JTAG like a real npm module
import jtag from '@continuum/jtag';

console.log('✅ JTAG Demo script loaded, jtag imported:', typeof jtag);

let browserUUID: any = null;
let jtagConnected = false;
let serverUUID: string | null = null;

// Initialize the demo when page loads
document.addEventListener('DOMContentLoaded', async () => {
  console.log('🎪 JTAG Demo: Page loaded');
  
  try {
    // Connect to JTAG system and get status
    const jtagSystem = await jtag.connect();
    jtagConnected = true;
    browserUUID = { uuid: jtagSystem.context.uuid, context: 'browser' };
    
    console.log('✅ JTAG Demo: JTAG SDK connected successfully');
  } catch (error) {
    console.error('❌ JTAG Demo: Failed to connect to JTAG SDK:', error);
    jtagConnected = false;
  }
  
  initializeDemo();
});

// JTAG client initialization is now handled in DOMContentLoaded above

function initializeDemo() {
  console.log('🔧 JTAG Demo: Initializing UI...');
  
  // Set up UI elements
  updateServerUUID();
  updateConnectionStatus();
  
  // Set up event handlers for buttons
  setupEventHandlers();
  
  console.log('✅ JTAG Demo: UI initialized');
}

function setupEventHandlers() {
  // Server-side buttons
  const serverLogBtn = document.getElementById('btn-server-log');
  const serverExecBtn = document.getElementById('btn-server-exec');  
  const serverScreenshotBtn = document.getElementById('btn-server-screenshot');
  
  // Browser-side buttons  
  const browserLogBtn = document.getElementById('btn-browser-log');
  const browserExecBtn = document.getElementById('btn-browser-exec');
  const browserScreenshotBtn = document.getElementById('btn-browser-screenshot');
  
  // Cross-context buttons
  const crossContextBtn = document.getElementById('btn-cross-context');
  const clearLogsBtn = document.getElementById('btn-clear-logs');
  
  // Attach clean event listeners
  serverLogBtn?.addEventListener('click', testServerLogging);
  serverExecBtn?.addEventListener('click', testServerExec);
  serverScreenshotBtn?.addEventListener('click', testServerScreenshot);
  
  browserLogBtn?.addEventListener('click', testBrowserLogging);
  browserExecBtn?.addEventListener('click', testBrowserExec);  
  browserScreenshotBtn?.addEventListener('click', testBrowserScreenshot);
  
  crossContextBtn?.addEventListener('click', testCrossContext);
  clearLogsBtn?.addEventListener('click', clearAllLogs);
  
  console.log('📋 JTAG Demo: Event handlers set up');
}

function updateServerUUID() {
  const serverUuidElement = document.getElementById('server-uuid');
  if (serverUuidElement) {
    serverUuidElement.textContent = 'Loading from server...';
  }
}

function updateBrowserUUID() {
  const browserUuidElement = document.getElementById('browser-uuid');
  if (browserUuidElement && browserUUID) {
    browserUuidElement.textContent = browserUUID.uuid;
  }
}

function updateConnectionStatus() {
  const statusElement = document.getElementById('connection-status');
  if (statusElement) {
    if (jtagConnected) {
      statusElement.textContent = 'Connected';
      statusElement.className = 'status connected';
    } else {
      statusElement.textContent = 'Disconnected';
      statusElement.className = 'status disconnected';
    }
  }
}

// Clean event handler functions - no globals, no window pollution
function testServerLogging() {
  console.log('🖥️ Testing server logging...');
  appendToLog('server-log', 'Server logging test initiated');
  
  try {
    jtag.log('DEMO_SERVER_TEST', 'Server logging test from browser');
    appendToLog('server-log', 'Server log sent via JTAG SDK');
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('server-log', `Server logging failed: ${errorMsg}`);
  }
}

async function testServerExec() {
  console.log('🖥️ Testing server exec...');
  appendToLog('server-log', 'Server exec test initiated');
  
  try {
    const result = await jtag.exec('console.log("Server exec test")');
    appendToLog('server-log', `Exec result: ${JSON.stringify(result)}`);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('server-log', `Server exec failed: ${errorMsg}`);
  }
}

async function testServerScreenshot() {
  console.log('🖥️ Testing server screenshot...');
  appendToLog('server-log', 'Server screenshot test initiated');
  
  try {
    const result = await jtag.screenshot({ filename: 'demo-test.png' });
    appendToLog('server-log', `Screenshot result: ${JSON.stringify(result)}`);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('server-log', `Server screenshot failed: ${errorMsg}`);
  }
}

function testBrowserLogging() {
  console.log('🌐 Testing browser logging...');
  appendToLog('browser-log', 'Browser logging test initiated');
  
  jtag.log('DEMO_BROWSER_TEST', 'Browser logging test');
  
  // Test console interception
  console.log('This should be intercepted by JTAG');
  console.error('This error should be intercepted by JTAG');
  console.warn('This warning should be intercepted by JTAG');
}

async function testBrowserExec() {
  console.log('🌐 Testing browser exec...');
  appendToLog('browser-log', 'Browser exec test initiated');
  
  try {
    const result = await jtag.exec('document.title');
    appendToLog('browser-log', `Exec result: ${JSON.stringify(result)}`);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('browser-log', `Browser exec failed: ${errorMsg}`);
  }
}

async function testBrowserScreenshot() {
  console.log('🌐 Testing browser screenshot...');
  appendToLog('browser-log', 'Browser screenshot test initiated');
  
  try {
    const result = await jtag.screenshot({ filename: 'browser-demo-test.png' });
    appendToLog('browser-log', `Screenshot result: ${JSON.stringify(result)}`);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('browser-log', `Browser screenshot failed: ${errorMsg}`);
  }
}

function testCrossContext() {
  console.log('🔄 Testing cross-context communication...');
  appendToLog('cross-context-log', 'Cross-context test initiated');
  
  jtag.log('CROSS_CONTEXT_TEST', 'Browser sending message to server', {
    testType: 'cross-context',
    timestamp: new Date().toISOString(),
    browserUUID: browserUUID?.uuid
  });
  
  appendToLog('cross-context-log', 'Message sent from browser to server');
}

function clearAllLogs() {
  console.log('🧹 Clearing all logs...');
  
  const logElements = ['server-log', 'browser-log', 'cross-context-log'];
  logElements.forEach(id => {
    const element = document.getElementById(id);
    if (element) {
      element.textContent = `${id.replace('-log', '').replace('-', ' ')} logs cleared...`;
    }
  });
}

function appendToLog(logId: string, message: string) {
  const logElement = document.getElementById(logId);
  if (logElement) {
    const timestamp = new Date().toLocaleTimeString();
    const logEntry = `[${timestamp}] ${message}\n`;
    logElement.textContent = (logElement.textContent || '') + logEntry;
    logElement.scrollTop = logElement.scrollHeight;
  }
}


console.log('📜 JTAG Demo: Script loaded and ready');

// Export to make this file a module (required for global declarations)
export {};