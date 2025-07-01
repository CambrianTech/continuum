/**
 * HTML Rendering Engine - Pure rendering logic
 * Focused on template loading and HTML generation
 */

import * as fs from 'fs';
import * as path from 'path';

// Get current directory for template resolution - ES module compatible
const __dirname = path.dirname(new URL(import.meta.url).pathname);

export interface RenderOptions {
  version: string;
  templatePath?: string;
}

export class HTMLRenderingEngine {
  private templateCache = new Map<string, string>();

  async renderMainUI(options: RenderOptions): Promise<string> {
    const templatePath = options.templatePath || this.getDefaultTemplatePath();
    
    if (!this.templateCache.has(templatePath)) {
      const template = fs.readFileSync(templatePath, 'utf8');
      this.templateCache.set(templatePath, template);
    }

    const template = this.templateCache.get(templatePath)!;
    
    return template
      .replace(/\{\{CONTINUUM_VERSION\}\}/g, options.version)
      .replace('</body>', this.generateScriptInjection(options) + '\n</body>');
  }

  async renderErrorPage(errorMessage: string, stackTrace: string): Promise<string> {
    const templatePath = path.join(__dirname, '../templates/error-page.html');
    
    try {
      const template = fs.readFileSync(templatePath, 'utf8');
      return template
        .replace('{{ERROR_MESSAGE}}', errorMessage)
        .replace('{{TIMESTAMP}}', new Date().toISOString())
        .replace('{{WORKING_DIR}}', process.cwd())
        .replace('{{STACK_TRACE}}', stackTrace);
    } catch (templateError) {
      return this.generateFallbackErrorPage(errorMessage, stackTrace);
    }
  }

  private getDefaultTemplatePath(): string {
    return path.join(__dirname, '../templates/main-ui.html');
  }

  private generateScriptInjection(options: RenderOptions): string {
    const apiPath = this.getApiPath();
    return `
    <!-- Dynamically injected by HTMLRenderingEngine -->
    <script>
        window.__CONTINUUM_VERSION__ = '${options.version}';
    </script>
    <script type="module" src="/dist/ui/continuum-browser.js"></script>`;
  }

  private getApiPath(): string {
    // Smart context detection like RendererDaemon
    const searchDir = __dirname;
    for (let i = 0; i < 10; i++) {
      try {
        const packagePath = path.join(searchDir, 'package.json');
        if (fs.existsSync(packagePath)) {
          const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
          if (packageData.name === 'continuum') {
            const isRepo = fs.existsSync(path.join(searchDir, 'src'));
            return isRepo ? '/node_modules/continuum/dist/api.js' : '/dist/api.js';
          }
        }
      } catch (error) {
        // Continue searching
      }
      const parentDir = path.dirname(searchDir);
      if (parentDir === searchDir) break;
    }
    return '/dist/api.js'; // Fallback
  }

  private generateFallbackErrorPage(errorMessage: string, stackTrace: string): string {
    return `<!DOCTYPE html>
<html><head><title>Continuum - Critical System Failure</title></head>
<body style="font-family: monospace; background: #111; color: #ff6666; padding: 20px;">
<h1>🚨 CRITICAL SYSTEM FAILURE</h1>
<p>Primary UI failed: ${errorMessage}</p>
<p>Time: ${new Date().toISOString()}</p>
<pre>${stackTrace}</pre>
</body></html>`;
  }
}