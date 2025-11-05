/**
 * HTMLRenderingEngine - Generates HTML for the UI
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export class HTMLRenderingEngine {
  async renderMainUI(options: { version?: string; templatePath?: string } = {}): Promise<string> {
    const { version = 'unknown' } = options;
    
    // Try to load the template file
    try {
      const templatePath = path.join(__dirname, '../templates/main-ui.html');
      let html = fs.readFileSync(templatePath, 'utf-8');
      
      // Inject version and scripts
      const scriptsToInject = `
    <!-- Dynamically injected by HTMLRenderingEngine -->
    <script>
        window.__CONTINUUM_VERSION__ = '${version}';
        window.__CONTINUUM_API_PATH__ = '/dist/api.js';
    </script>
    <script type="module" src="/src/ui/continuum-browser.js"></script>`;
      
      // Insert scripts before closing body tag
      html = html.replace('</body>', scriptsToInject + '\n</body>');
      
      return html;
    } catch (error) {
      console.warn('Failed to load template, using fallback HTML:', error);
      // Fallback to inline HTML
    }
    
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Continuum v${version}</title>
    
    <!-- Define version for browser API -->
    <script>
        window.__CONTINUUM_VERSION__ = '${version}';
    </script>
    
    <style>
        body {
            margin: 0;
            padding: 0;
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            background: #0a0a0a;
            color: #e0e0e0;
            overflow: hidden;
            height: 100vh;
        }
        
        .continuum-container {
            display: flex;
            height: 100vh;
            position: relative;
        }
        
        /* Loading state */
        .loading-overlay {
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: rgba(0, 0, 0, 0.8);
            display: flex;
            align-items: center;
            justify-content: center;
            z-index: 9999;
            transition: opacity 0.3s ease;
        }
        
        .loading-overlay.hidden {
            opacity: 0;
            pointer-events: none;
        }
        
        .loading-content {
            text-align: center;
        }
        
        .loading-spinner {
            width: 50px;
            height: 50px;
            border: 3px solid rgba(255, 255, 255, 0.3);
            border-radius: 50%;
            border-top-color: #3490dc;
            animation: spin 1s ease-in-out infinite;
        }
        
        @keyframes spin {
            to { transform: rotate(360deg); }
        }
    </style>
</head>
<body>
    <div class="continuum-container">
        <!-- Sidebar Widget -->
        <continuum-sidebar></continuum-sidebar>
        
        <!-- Chat Widget -->
        <chat-widget></chat-widget>
    </div>
    
    <!-- Loading Overlay -->
    <div id="loading-overlay" class="loading-overlay">
        <div class="loading-content">
            <div class="loading-spinner"></div>
            <p style="margin-top: 20px; color: #888;">Initializing Continuum...</p>
        </div>
    </div>
    
    <!-- Load browser API and widgets -->
    <script type="module">
        // Import and initialize the browser API
        import('/src/ui/continuum-browser.js').then(module => {
            console.log('🌐 Continuum Browser API loaded');
            
            // Hide loading overlay once API is ready
            window.addEventListener('continuum:ready', () => {
                const overlay = document.getElementById('loading-overlay');
                if (overlay) {
                    overlay.classList.add('hidden');
                }
            });
        }).catch(error => {
            console.error('Failed to load Continuum browser API:', error);
        });
    </script>
</body>
</html>`;
  }
  
  async renderErrorPage(errorMessage: string, stackTrace: string): Promise<string> {
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Continuum - Error</title>
    <style>
        body {
            margin: 0;
            padding: 20px;
            font-family: monospace;
            background: #1a1a1a;
            color: #ff6b6b;
        }
        .error-container {
            max-width: 800px;
            margin: 0 auto;
            padding: 20px;
            background: #2a2a2a;
            border-radius: 8px;
            border: 1px solid #ff6b6b;
        }
        h1 { color: #ff6b6b; }
        pre {
            background: #1a1a1a;
            padding: 15px;
            border-radius: 4px;
            overflow-x: auto;
            white-space: pre-wrap;
        }
    </style>
</head>
<body>
    <div class="error-container">
        <h1>🚨 Continuum Error</h1>
        <p>${errorMessage}</p>
        <h2>Stack Trace:</h2>
        <pre>${stackTrace}</pre>
    </div>
</body>
</html>`;
  }
}