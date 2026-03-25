/**
 * Continuum Widget UI - Simplified Interface
 */

// Reduce log spam
// console.log('🚀 Widget UI: Loading...');

// Import JTAG (browser build will automatically use browser-index via package.json)
import { jtag } from '@continuum/jtag';
import type { JTAGClient } from '@continuum/jtag/dist/system/core/client/shared/JTAGClient';

// Import widget components
import './components/ContinuumEmoter.js';
import './components/PanelResizer.js';  // Unified resizer for both sidebars

// console.log('✅ Widget UI: JTAG imported:', typeof jtag);

// Cross-platform environment detection
const isBrowser = typeof window !== 'undefined' && typeof document !== 'undefined';

// Safe document access
declare const document: Document;

// Extend window interface for widget functions
interface WindowWithWidgetFunctions extends Window {
  takeScreenshot: () => Promise<void>;
  clearWidgetLog: () => void;
}

declare const window: WindowWithWidgetFunctions;

let jtagConnected = false;
let jtagClient: JTAGClient | null = null;

// Initialize the widget UI when page loads (only in browser)
if (isBrowser) {
  document.addEventListener('DOMContentLoaded', async () => {
    // console.log('🎪 Widget UI: Page loaded');

    try {
      // Connect to JTAG client and get status
      const connectionResult = await jtag.connect();
      jtagClient = connectionResult.client;
      jtagConnected = true;

      // Set up global window.jtag for backwards compatibility with tests
      window.jtag = jtagClient;


      // console.log('✅ Widget UI: JTAG Client connected successfully');
    } catch (error) {
      console.error('❌ Widget UI: Failed to connect to JTAG Client:', error);
      jtagConnected = false;
    }
    
    initializeWidgetUI();
  });
}

function initializeWidgetUI(): void {
  console.log('🔧 Widget UI: Initializing...');
  
  // Update connection status display
  updateConnectionStatus();
  
  // Initial log message
  appendToWidgetLog('Widget UI initialized - ready for screenshots');
  
  console.log('✅ Widget UI: Initialized');
}

function updateConnectionStatus(): void {
  const statusElement = document.getElementById('connection-status');
  if (statusElement) {
    if (jtagConnected) {
      statusElement.textContent = 'Connected';
      statusElement.className = 'status connected';
    } else {
      statusElement.textContent = 'Initializing...';
      statusElement.className = 'status disconnected';
    }
  }
}

// ========================================
// 📸 WIDGET UI - SIMPLIFIED FUNCTIONS
// ========================================

/**
 * Take a screenshot using the widget interface
 */
async function takeScreenshot(): Promise<void> {
  console.log('📸 Widget UI: Taking screenshot...');
  appendToWidgetLog('📸 Capturing screenshot...');
  
  if (!jtagClient) {
    appendToWidgetLog('❌ JTAG Client not connected');
    return;
  }
  
  try {
    const result = await jtagClient.commands.screenshot({ 
      filename: `widget-screenshot-${Date.now()}.png`,
      selector: '.cyberpunk-container'
    });
    
    appendToWidgetLog(`✅ Screenshot captured: ${result.filepath || 'saved to session'}`);
    console.log('✅ Widget UI: Screenshot complete:', result);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    appendToWidgetLog(`❌ Screenshot failed: ${errorMsg}`);
    console.error('❌ Widget UI: Screenshot error:', error);
  }
}

/**
 * Clear the widget log display
 */
function clearWidgetLog(): void {
  console.log('🧹 Widget UI: Clearing log...');
  const logElement = document.getElementById('widget-log');
  if (logElement) {
    logElement.textContent = 'Widget log cleared...';
  }
}

// ========================================
// 🌐 GLOBAL FUNCTION EXPOSURE
// ========================================

// Make widget functions available globally for HTML onclick handlers
window.takeScreenshot = takeScreenshot;
window.clearWidgetLog = clearWidgetLog;

/**
 * Append message to widget log with timestamp
 */
function appendToWidgetLog(message: string): void {
  const logElement = document.getElementById('widget-log');
  if (logElement) {
    const timestamp = new Date().toLocaleTimeString();
    const logEntry = `[${timestamp}] ${message}\n`;
    logElement.textContent = (logElement.textContent ?? '') + logEntry;
    logElement.scrollTop = logElement.scrollHeight;
  }
}

console.log('📜 Widget UI: Script loaded and ready');

// Export to make this file a module (required for global declarations)
export {};