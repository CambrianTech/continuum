/**
 * Widget Discovery System - Finds and validates widgets using package.json
 * Follows same modular architecture as commands
 * Ensures all widget directories comply with structure requirements
 */

import * as fs from 'fs';
import * as path from 'path';

export interface WidgetMetadata {
  name: string;
  path: string;
  packageJson: any;
  widgetFile: string;
  testFiles: string[];
  cssFiles: string[];
  isCompliant: boolean;
  warnings: string[];
}

export class WidgetDiscovery {
  private componentsDir: string;

  constructor(componentsDir?: string) {
    this.componentsDir = componentsDir || path.join(process.cwd(), 'src/ui/components');
  }

  /**
   * Discover all widgets by scanning for package.json files
   */
  async discoverWidgets(): Promise<WidgetMetadata[]> {
    const widgets: WidgetMetadata[] = [];
    const directories = this.getDirectories(this.componentsDir);

    for (const dir of directories) {
      if (dir === 'shared') continue; // Skip shared utilities
      
      const widgetPath = path.join(this.componentsDir, dir);
      const packageJsonPath = path.join(widgetPath, 'package.json');
      
      if (fs.existsSync(packageJsonPath)) {
        try {
          const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
          const metadata = await this.analyzeWidget(dir, widgetPath, packageJson);
          widgets.push(metadata);
        } catch (error) {
          console.warn(`⚠️ Failed to parse package.json in ${dir}:`, error);
        }
      } else {
        console.warn(`⚠️ Widget directory ${dir} missing package.json - not discoverable`);
      }
    }

    return widgets;
  }

  /**
   * Analyze a single widget directory for compliance
   */
  private async analyzeWidget(name: string, widgetPath: string, packageJson: any): Promise<WidgetMetadata> {
    const warnings: string[] = [];
    let isCompliant = true;
    
    // Find widget implementation file
    const widgetFile = this.findWidgetFile(widgetPath, name);
    if (!widgetFile) {
      warnings.push(`No widget implementation file found (expected ${name}Widget.ts)`);
      isCompliant = false;
    }

    // Find test files
    const testFiles = this.findTestFiles(widgetPath);
    if (testFiles.length === 0) {
      warnings.push('No test files found - should have unit tests');
      isCompliant = false;
    }

    // Find CSS files
    const cssFiles = this.findCSSFiles(widgetPath);

    // Validate package.json structure
    if (!packageJson.name) {
      warnings.push('package.json missing name field');
      isCompliant = false;
    }

    if (!packageJson.main && !widgetFile) {
      warnings.push('package.json missing main field and no widget file found');
      isCompliant = false;
    }

    return {
      name,
      path: widgetPath,
      packageJson,
      widgetFile: widgetFile || '',
      testFiles,
      cssFiles,
      isCompliant,
      warnings
    };
  }

  private findWidgetFile(widgetPath: string, name: string): string | null {
    const possibleFiles = [
      `${name}Widget.ts`,
      `${name}.ts`,
      'index.ts'
    ];

    for (const file of possibleFiles) {
      const filePath = path.join(widgetPath, file);
      if (fs.existsSync(filePath)) {
        return file;
      }
    }

    return null;
  }

  private findTestFiles(widgetPath: string): string[] {
    const testFiles: string[] = [];
    const testDir = path.join(widgetPath, 'test');
    
    if (fs.existsSync(testDir)) {
      const files = fs.readdirSync(testDir, { recursive: true });
      for (const file of files) {
        if (typeof file === 'string' && file.endsWith('.test.ts')) {
          testFiles.push(path.join('test', file));
        }
      }
    }

    // Also check for test files in root
    const files = fs.readdirSync(widgetPath);
    for (const file of files) {
      if (file.endsWith('.test.ts')) {
        testFiles.push(file);
      }
    }

    return testFiles;
  }

  private findCSSFiles(widgetPath: string): string[] {
    const cssFiles: string[] = [];
    const files = fs.readdirSync(widgetPath);
    
    for (const file of files) {
      if (file.endsWith('.css')) {
        cssFiles.push(file);
      }
    }

    return cssFiles;
  }

  private getDirectories(dirPath: string): string[] {
    if (!fs.existsSync(dirPath)) {
      return [];
    }

    return fs.readdirSync(dirPath, { withFileTypes: true })
      .filter(dirent => dirent.isDirectory())
      .map(dirent => dirent.name);
  }

  /**
   * Generate browser-accessible widget paths
   */
  generateWidgetPaths(widgets: WidgetMetadata[]): string[] {
    return widgets
      .filter(w => w.isCompliant && w.widgetFile)
      .map(w => `/dist/ui/components/${w.name}/${w.widgetFile.replace('.ts', '.js')}`);
  }

  /**
   * Validate all widgets and report compliance
   */
  validateAllWidgets(): Promise<{ compliant: WidgetMetadata[], nonCompliant: WidgetMetadata[] }> {
    return this.discoverWidgets().then(widgets => {
      const compliant = widgets.filter(w => w.isCompliant);
      const nonCompliant = widgets.filter(w => !w.isCompliant);

      if (nonCompliant.length > 0) {
        console.warn(`⚠️ Found ${nonCompliant.length} non-compliant widget directories:`);
        for (const widget of nonCompliant) {
          console.warn(`  - ${widget.name}: ${widget.warnings.join(', ')}`);
        }
      }

      console.log(`✅ Found ${compliant.length} compliant widgets`);
      return { compliant, nonCompliant };
    });
  }
}