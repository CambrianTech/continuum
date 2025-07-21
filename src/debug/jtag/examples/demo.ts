/**
 * JTAG End-to-End Demo - Client-side TypeScript
 */

let browserUUID: any = null;
let jtagConnected = false;
let serverUUID: string | null = null;

// Initialize the demo when page loads
document.addEventListener('DOMContentLoaded', () => {
  console.log('🎪 JTAG Demo: Page loaded');
  initializeDemo();
});

// Wait for JTAG to be ready
window.addEventListener('jtag:ready', (event: any) => {
  console.log('✅ JTAG Demo: JTAG ready event received', event.detail);
  jtagConnected = true;
  updateConnectionStatus();
  
  if (window.jtag) {
    browserUUID = window.jtag.getUUID();
    updateBrowserUUID();
    
    // Test initial connection
    testBrowserLogging();
  }
});

window.addEventListener('jtag:error', (event: any) => {
  console.error('❌ JTAG Demo: JTAG error event', event.detail);
  jtagConnected = false;
  updateConnectionStatus();
});

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
  const serverLogBtn = document.querySelector('button[onclick="testServerLogging()"]');
  const serverExecBtn = document.querySelector('button[onclick="testServerExec()"]');
  const serverScreenshotBtn = document.querySelector('button[onclick="testServerScreenshot()"]');
  
  // Browser-side buttons  
  const browserLogBtn = document.querySelector('button[onclick="testBrowserLogging()"]');
  const browserExecBtn = document.querySelector('button[onclick="testBrowserExec()"]');
  const browserScreenshotBtn = document.querySelector('button[onclick="testBrowserScreenshot()"]');
  
  // Cross-context buttons
  const crossContextBtn = document.querySelector('button[onclick="testCrossContext()"]');
  const clearLogsBtn = document.querySelector('button[onclick="clearAllLogs()"]');
  
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

// Global functions for button onclick handlers
(window as any).testServerLogging = function() {
  console.log('🖥️ Testing server logging...');
  appendToLog('server-log', 'Server logging test initiated');
  
  if (window.jtag) {
    window.jtag.log('DEMO_SERVER_TEST', 'Server logging test from browser');
  }
};

(window as any).testServerExec = function() {
  console.log('🖥️ Testing server exec...');
  appendToLog('server-log', 'Server exec test initiated');
  
  if (window.jtag) {
    window.jtag.exec('console.log("Server exec test")').then((result: any) => {
      appendToLog('server-log', `Exec result: ${JSON.stringify(result)}`);
    });
  }
};

(window as any).testServerScreenshot = function() {
  console.log('🖥️ Testing server screenshot...');
  appendToLog('server-log', 'Server screenshot test initiated');
  
  if (window.jtag) {
    window.jtag.screenshot('demo-test').then((result: any) => {
      appendToLog('server-log', `Screenshot result: ${JSON.stringify(result)}`);
    });
  }
};

(window as any).testBrowserLogging = function() {
  console.log('🌐 Testing browser logging...');
  appendToLog('browser-log', 'Browser logging test initiated');
  
  if (window.jtag) {
    window.jtag.log('DEMO_BROWSER_TEST', 'Browser logging test');
    window.jtag.critical('DEMO_BROWSER_CRITICAL', 'Browser critical test');
  }
  
  // Test console interception
  console.log('This should be intercepted by JTAG');
  console.error('This error should be intercepted by JTAG');
  console.warn('This warning should be intercepted by JTAG');
};

(window as any).testBrowserExec = function() {
  console.log('🌐 Testing browser exec...');
  appendToLog('browser-log', 'Browser exec test initiated');
  
  if (window.jtag) {
    window.jtag.exec('document.title').then((result: any) => {
      appendToLog('browser-log', `Exec result: ${JSON.stringify(result)}`);
    });
  }
};

(window as any).testBrowserScreenshot = function() {
  console.log('🌐 Testing browser screenshot...');
  appendToLog('browser-log', 'Browser screenshot test initiated');
  
  if (window.jtag) {
    window.jtag.screenshot('browser-demo-test').then((result: any) => {
      appendToLog('browser-log', `Screenshot result: ${JSON.stringify(result)}`);
    });
  }
};

(window as any).testCrossContext = function() {
  console.log('🔄 Testing cross-context communication...');
  appendToLog('cross-context-log', 'Cross-context test initiated');
  
  // Test browser-to-server communication
  if (window.jtag) {
    window.jtag.log('CROSS_CONTEXT_TEST', 'Browser sending message to server', {
      testType: 'cross-context',
      timestamp: new Date().toISOString(),
      browserUUID: browserUUID?.uuid
    });
    
    appendToLog('cross-context-log', 'Message sent from browser to server');
  }
};

(window as any).clearAllLogs = function() {
  console.log('🧹 Clearing all logs...');
  
  const logElements = ['server-log', 'browser-log', 'cross-context-log'];
  logElements.forEach(id => {
    const element = document.getElementById(id);
    if (element) {
      element.textContent = `${id.replace('-log', '').replace('-', ' ')} logs cleared...`;
    }
  });
};

function appendToLog(logId: string, message: string) {
  const logElement = document.getElementById(logId);
  if (logElement) {
    const timestamp = new Date().toLocaleTimeString();
    const logEntry = `[${timestamp}] ${message}\n`;
    logElement.textContent = (logElement.textContent || '') + logEntry;
    logElement.scrollTop = logElement.scrollHeight;
  }
}

// Global type declarations
declare global {
  interface Window {
    jtag: {
      log: (component: string, message: string, data?: any) => void;
      critical: (component: string, message: string, data?: any) => void;
      exec: (code: string) => Promise<any>;
      screenshot: (filename: string) => Promise<any>;
      getUUID: () => { uuid: string; context: string; timestamp: string };
    };
  }
}

console.log('📜 JTAG Demo: Script loaded and ready');