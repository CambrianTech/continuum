/**
 * Screenshot Module - Client Entry Point
 * Self-contained browser-side functionality with proper ES modules
 */

// Dynamic client-side loading
import { ScreenshotUtils } from './ScreenshotUtils.js';

// Module metadata (would come from package.json in real setup)
const VERSION = '1.2.0';

export default {
    // Client utilities
    utils: ScreenshotUtils,
    
    // Module info
    name: 'screenshot',
    version: VERSION,
    description: 'Screenshot system browser components',
    
    // Client-specific metadata
    type: 'client',
    capabilities: ['html2canvas', 'screenshot', 'websocket'],
    
    // Files in this client module
    files: ['ScreenshotUtils.js', 'ScreenshotCommand.client.js'],
    
    // Initialization
    async initialize() {
        console.log(`📦 Screenshot module (client) v${VERSION} initialized`);
        
        // Register with global continuum if available
        if (typeof window !== 'undefined' && window.continuum) {
            window.continuum.modules = window.continuum.modules || {};
            window.continuum.modules.screenshot = this;
        }
        
        return true;
    },
    
    // Check if all dependencies are loaded
    checkDependencies() {
        return {
            html2canvas: typeof html2canvas !== 'undefined',
            websocket: !!(window.ws && window.ws.readyState === WebSocket.OPEN),
            utils: !!ScreenshotUtils
        };
    }
};