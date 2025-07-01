/**
 * TypeScript Compiler - Handles TypeScript to JavaScript compilation
 * Uses real TypeScript compiler instead of fragile regex
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import * as ts from 'typescript';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export interface CompilationOptions {
  version: string;
  sourceMap?: boolean;
  minify?: boolean;
}

export class TypeScriptCompiler {
  
  async compileContinuumAPI(options: CompilationOptions): Promise<string> {
    const tsSource = this.loadTypeScriptSource();
    return this.compileTypeScript(tsSource, options);
  }

  async compileBrowserScript(options: CompilationOptions): Promise<string> {
    // Same as compileContinuumAPI - they both compile continuum-browser.ts
    const tsSource = this.loadTypeScriptSource();
    return this.compileTypeScript(tsSource, options);
  }

  async compileWidgetComponent(source: string, _filePath: string): Promise<string> {
    // Compile widget TypeScript to JavaScript for browser
    const compilerOptions: ts.CompilerOptions = {
      target: ts.ScriptTarget.ES2020,
      module: ts.ModuleKind.ES2020,
      moduleResolution: ts.ModuleResolutionKind.NodeJs,
      strict: false,
      skipLibCheck: true,
      removeComments: false,
      allowSyntheticDefaultImports: true
    };

    // Transform imports to be browser-compatible
    let browserSource = source
      .replace(/from ['"]\.\/([^'"]+)\.ts['"]/g, "from './$1.js'")  // .ts -> .js
      .replace(/from ['"]\.\.\/([^'"]+)\.ts['"]/g, "from '../$1.js'")  // relative .ts -> .js
      .replace(/from ['"]([^'"]+)\.css['"]/g, "// CSS import: $1.css");  // remove CSS imports for now

    const result = ts.transpile(browserSource, compilerOptions);
    return result;
  }

  private loadTypeScriptSource(): string {
    const moduleDir = __dirname;
    const tsSource = path.join(moduleDir, '../../../ui/continuum-browser.ts');
    return fs.readFileSync(tsSource, 'utf-8');
  }

  private compileTypeScript(source: string, options: CompilationOptions): string {
    // Configure TypeScript for clean browser output
    const compilerOptions: ts.CompilerOptions = {
      target: ts.ScriptTarget.ES2020,
      module: ts.ModuleKind.None,  // No module system = plain browser JS
      strict: false,
      skipLibCheck: true,
      removeComments: false
    };

    // Prepare source for browser: remove exports/imports before compilation
    let browserSource = source
      .replace(/export\s+/g, '') // Remove export keywords
      .replace(/import\s+.*?from\s+.*?;?\s*/g, '') // Remove import statements
      .replace(/\{\{CONTINUUM_VERSION\}\}/g, options.version);

    // Transpile with TypeScript - should produce clean browser JS
    const result = ts.transpile(browserSource, compilerOptions);
    
    // Add browser integration
    const compiled = result + `

// Dynamic widget loading for modular architecture  
if (window.continuum) {
  window.continuum.loadWidgets = async function() {
    console.log('🔍 Loading widgets dynamically...');
    // Widget loading will be implemented based on discovered modules
  };
}`;
    
    return compiled;
  }
}