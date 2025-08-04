/**
 * JTAG End-to-End Demo - Client-side TypeScript with Universal JTAG
 */

console.log('🚀 JTAG Demo script loading...');

// Import JTAG (browser build will automatically use browser-index via package.json)
import { jtag } from '@continuum/jtag';
import type { JTAGClient } from '@continuum/jtag/dist/system/core/client/shared/JTAGClient';

console.log('✅ JTAG Demo script loaded, jtag imported:', typeof jtag);

// Cross-platform environment detection
const isBrowser = typeof window !== 'undefined' && typeof document !== 'undefined';

// Safe document access
declare const document: Document;
// eslint-disable-next-line @typescript-eslint/naming-convention
declare const Image: new() => HTMLImageElement;

// Extend window interface for our demo functions
interface WindowWithDemoFunctions extends Window {
  testServerLogging: () => void;
  testServerExec: () => Promise<void>;
  testServerScreenshot: () => Promise<void>;
  testBrowserLogging: () => void;
  testBrowserExec: () => Promise<void>;
  testBrowserScreenshot: () => Promise<void>;
  testCrossContext: () => void;
  clearAllLogs: () => void;
  demoScreenshotWidget: () => Promise<void>;
  demoCropAndManipulate: () => Promise<void>;
  demoDownloadScreenshot: () => Promise<void>;
}

declare const window: WindowWithDemoFunctions;

let sessionUUID = "FIX ME";
let jtagConnected = false;
let jtagClient: JTAGClient | null = null;

// Initialize the demo when page loads (only in browser)
if (isBrowser) {
  document.addEventListener('DOMContentLoaded', async () => {
    console.log('🎪 JTAG Demo: Page loaded');
    
    try {
      // Connect to JTAG client and get status
      const connectionResult = await jtag.connect();
      jtagClient = connectionResult.client;
      jtagConnected = true;
      
      console.log('✅ JTAG Demo: JTAG Client connected successfully');
    } catch (error) {
      console.error('❌ JTAG Demo: Failed to connect to JTAG Client:', error);
      jtagConnected = false;
    }
    
    initializeDemo();
  });
}

// JTAG client initialization is now handled in DOMContentLoaded above

function initializeDemo(): void {
  console.log('🔧 JTAG Demo: Initializing UI...');
  
  // Set up UI elements
  updateServerUUID();
  updateConnectionStatus();
  
  // Set up event handlers for buttons
  setupEventHandlers();
  
  console.log('✅ JTAG Demo: UI initialized');
}

function setupEventHandlers(): void {
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

function updateServerUUID(): void {
  const serverUuidElement = document.getElementById('server-uuid');
  if (serverUuidElement) {
    serverUuidElement.textContent = 'Loading from server...';
  }
}

function updateConnectionStatus(): void {
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
function testServerLogging(): void {
  console.log('🖥️ Testing server logging...');
  appendToLog('server-log', 'Server logging test initiated');
  
  try {
    console.log('DEMO_SERVER_TEST', 'Server logging test from browser');
    appendToLog('server-log', 'Server log sent via JTAG SDK');
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('server-log', `Server logging failed: ${errorMsg}`);
  }
}

async function testServerExec(): Promise<void> {
  console.log('🖥️ Testing server exec...');
  appendToLog('server-log', 'Server exec test initiated');
  
  try {
    const result = await jtagClient.commands.exec('console.log("Server exec test")');
    appendToLog('server-log', `Exec result: ${JSON.stringify(result)}`);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('server-log', `Server exec failed: ${errorMsg}`);
  }
}

async function testServerScreenshot(): Promise<void> {
  console.log('🖥️ Testing server screenshot...');
  appendToLog('server-log', 'Server screenshot test initiated');
  
  try {
    const result = await jtagClient.commands.screenshot({ filename: 'demo-test.png' });
    appendToLog('server-log', `Screenshot result: ${JSON.stringify(result)}`);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('server-log', `Server screenshot failed: ${errorMsg}`);
  }
}

const testBrowserLogging = (): void => {
  console.log('🌐 Testing browser logging...');
  appendToLog('browser-log', 'Browser logging test initiated');
  
  jtagClient?.commands.log({
    category: 'DEMO_BROWSER_TEST',
    message: 'Browser logging test'
  });
  
  // Test console interception
  console.log('This should be intercepted by JTAG');
  console.error('This error should be intercepted by JTAG');
  console.warn('This warning should be intercepted by JTAG');
}

const testBrowserExec = async (): Promise<void> => {
  console.log('🌐 Testing browser exec...');
  appendToLog('browser-log', 'Browser exec test initiated');
  
  try {
    const result = await jtagClient.commands.exec('document.title');
    appendToLog('browser-log', `Exec result: ${JSON.stringify(result)}`);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('browser-log', `Browser exec failed: ${errorMsg}`);
  }
}

const testBrowserScreenshot = async (): Promise<void> => {
  console.log('🌐 Testing browser screenshot...');
  appendToLog('browser-log', 'Browser screenshot test initiated');
  
  try {
    console.log('🔍 JTAG Client available:', !!jtagClient);
    console.log('🔍 Screenshot command available:', typeof jtagClient?.commands?.screenshot);
    
    const result = await jtagClient.commands.screenshot({ filename: 'browser-demo-test.png' });
    appendToLog('browser-log', `Screenshot result: ${JSON.stringify(result)}`);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    console.error('❌ Screenshot error details:', error);
    appendToLog('browser-log', `Browser screenshot failed: ${errorMsg}`);
  }
}

// ========================================
// 📸 SCREENSHOT WIDGET DEMO FUNCTIONS
// ========================================

/**
 * Demo: Screenshot with different parameter behaviors
 * Shows the modular parametric design in action
 */
const demoScreenshotWidget = async (): Promise<void> => {
  console.log('📸 Screenshot Widget Demo: Starting parametric demo...');
  appendToLog('widget-log', '🎯 Starting Screenshot Widget Demo - showcasing parametric behavior');
  
  try {
    // Demo 1: Basic screenshot with session-aware saving
    appendToLog('widget-log', '📷 Demo 1: Basic screenshot → server save');
    const basicResult = await jtagClient.commands.screenshot({
      filename: `widget-demo-${Date.now()}.png`,
      selector: '.browser-panel', // Target specific element
      returnFormat: 'file'
    });
    appendToLog('widget-log', `✅ Saved to: ${basicResult.filepath ?? 'session directory'}`);

    // Demo 2: Get screenshot as bytes for manipulation
    appendToLog('widget-log', '📷 Demo 2: Screenshot → bytes for browser manipulation');
    const bytesResult = await jtagClient.commands.screenshot({
      filename: `widget-bytes-${Date.now()}.png`,
      selector: '.server-panel',
      returnFormat: 'bytes',
      crop: { x: 10, y: 10, width: 300, height: 200 }
    });
    appendToLog('widget-log', `✅ Got ${bytesResult.metadata?.size ?? 'unknown'} bytes for manipulation`);
    
    // Demo 3: High-quality JPEG with custom options
    appendToLog('widget-log', '📷 Demo 3: Custom format & quality');
    const customResult = await jtagClient.commands.screenshot({
      filename: `widget-custom-${Date.now()}.jpg`,
      selector: 'h1',
      options: {
        format: 'jpeg',
        quality: 0.95,
        backgroundColor: '#ffffff',
        scale: 2
      }
    });
    appendToLog('widget-log', `✅ Custom screenshot: ${customResult.metadata?.format} @ ${customResult.metadata?.quality ?? 'default'}% quality`);

    appendToLog('widget-log', '🎉 Screenshot Widget Demo Complete - All parameters worked!');
    
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('widget-log', `❌ Screenshot widget demo failed: ${errorMsg}`);
    console.error('Screenshot widget demo error:', error);
  }
}

/**
 * Demo: Screenshot cropping and browser manipulation
 */
async function demoCropAndManipulate(): Promise<void> {
  console.log('✂️ Crop & Manipulate Demo: Starting...');
  appendToLog('widget-log', '✂️ Demo: Crop & Manipulate - browser-side processing');
  
  try {
    // Get screenshot as bytes for manipulation
    const result = await jtagClient.commands.screenshot({
      filename: 'crop-demo.png',
      selector: '.container',
      returnFormat: 'bytes',
      crop: { x: 0, y: 0, width: 400, height: 300 }
    });
    
    if (result.dataUrl) {
      // Simulate browser-side manipulation
      appendToLog('widget-log', '🎨 Processing image in browser...');
      
      // Create canvas for manipulation
      const canvas = document.createElement('canvas');
      const ctx = canvas.getContext('2d');
      const img = new Image();
      
      img.onload = (): void => {
        canvas.width = img.width;
        canvas.height = img.height;
        
        // Draw original
        ctx!.drawImage(img, 0, 0);
        
        // Add overlay text (demo manipulation)
        ctx!.fillStyle = 'rgba(255, 0, 0, 0.8)';
        ctx!.font = 'bold 16px Arial';
        ctx!.fillText('PROCESSED BY WIDGET', 10, 30);
        
        // Convert back to dataUrl
        const processedDataUrl = canvas.toDataURL('image/png');
        
        // Now save the processed version
        jtagClient.commands.screenshot({
          filename: `processed-${Date.now()}.png`,
          dataUrl: processedDataUrl, // Pass processed data
          returnFormat: 'file'
        }).then(() => {
          appendToLog('widget-log', '✅ Processed screenshot saved to server');
        });
      };
      
      img.src = result.dataUrl;
      appendToLog('widget-log', '🖼️ Browser manipulation complete');
    }
    
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('widget-log', `❌ Crop & manipulate demo failed: ${errorMsg}`);
  }
}

/**
 * Demo: Download screenshot directly in browser
 */
async function demoDownloadScreenshot(): Promise<void> {
  console.log('⬇️ Download Demo: Starting...');
  appendToLog('widget-log', '⬇️ Demo: Direct browser download');
  
  try {
    const result = await jtagClient.commands.screenshot({
      filename: `download-demo-${Date.now()}.png`,
      selector: '.browser-panel',
      returnFormat: 'download' // This should trigger browser download
    });
    
    // Fallback: manual download if returnFormat not implemented
    if (result.dataUrl) {
      const link = document.createElement('a');
      link.download = `manual-download-${Date.now()}.png`;
      link.href = result.dataUrl;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      
      appendToLog('widget-log', '✅ Screenshot downloaded to browser');
    }
    
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToLog('widget-log', `❌ Download demo failed: ${errorMsg}`);
  }
}

function testCrossContext(): void {
  console.log('🔄 Testing cross-context communication...');
  appendToLog('cross-context-log', 'Cross-context test initiated');
  
  jtagClient?.commands.log({
    category: 'CROSS_CONTEXT_TEST',
    message: 'Browser sending message to server',
    metadata: {
      testType: 'cross-context',
      timestamp: new Date().toISOString(),
    }
  });
  
  appendToLog('cross-context-log', 'Message sent from browser to server');
}

function clearAllLogs(): void {
  console.log('🧹 Clearing all logs...');
  
  const logElements = ['server-log', 'browser-log', 'cross-context-log', 'widget-log'];
  logElements.forEach(id => {
    const element = document.getElementById(id);
    if (element) {
      element.textContent = `${id.replace('-log', '').replace('-', ' ')} logs cleared...`;
    }
  });
}

// ========================================
// 🌐 GLOBAL FUNCTION EXPOSURE
// ========================================

// Make functions available globally for HTML onclick handlers
window.testServerLogging = testServerLogging;
window.testServerExec = testServerExec;
window.testServerScreenshot = testServerScreenshot;
window.testBrowserLogging = testBrowserLogging;
window.testBrowserExec = testBrowserExec;
window.testBrowserScreenshot = testBrowserScreenshot;
window.testCrossContext = testCrossContext;
window.clearAllLogs = clearAllLogs;

// Screenshot Widget Demo Functions
window.demoScreenshotWidget = demoScreenshotWidget;
window.demoCropAndManipulate = demoCropAndManipulate;
window.demoDownloadScreenshot = demoDownloadScreenshot;

function appendToLog(logId: string, message: string): void {
  const logElement = document.getElementById(logId);
  if (logElement) {
    const timestamp = new Date().toLocaleTimeString();
    const logEntry = `[${timestamp}] ${message}\n`;
    logElement.textContent = (logElement.textContent ?? '') + logEntry;
    logElement.scrollTop = logElement.scrollHeight;
  }
}


console.log('📜 JTAG Demo: Script loaded and ready');

// Export to make this file a module (required for global declarations)
export {};