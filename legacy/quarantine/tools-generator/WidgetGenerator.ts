/**
 * WidgetGenerator - Generate widget files with all wiring
 *
 * Generates:
 * - Widget TypeScript file (extends BaseWidget)
 * - HTML template file
 * - SCSS styles file (imports from _variables.scss)
 * - README documentation
 * - Recipe JSON file
 *
 * Uses template files from generator/templates/widget/ with token replacement.
 * Same pattern as CommandGenerator for consistency.
 *
 * Eliminates manual wiring pain by automating:
 * - Widget class with proper BaseWidget extension
 * - Positron context emission
 * - SCSS with shared variables
 * - Recipe for layout configuration
 * - README for AI discoverability
 */

import { ModuleGenerator, type GenerateOptions } from './ModuleGenerator';
import { TokenReplacer } from './TokenReplacer';
import * as path from 'path';
import * as fs from 'fs';

export interface WidgetSpec {
  /** Widget name in PascalCase WITHOUT "Widget" suffix, e.g., "WebView", "UserProfile", "Test" */
  name: string;
  /** Brief description of what this widget does */
  description: string;
  /** Display name shown in UI tabs */
  displayName: string;
  /** URL path prefix, e.g., "/browser", "/profile" */
  pathPrefix: string;
  /** Whether this widget needs an entityId (like userId, roomId) */
  requiresEntity: boolean;
  /** Right panel config: null = hidden, { room: 'help' } = show chat */
  rightPanel?: { room: string; compact?: boolean } | null;
  /** Initial placeholder content */
  placeholderTitle?: string;
  placeholderText?: string;
}

export class WidgetGenerator extends ModuleGenerator<WidgetSpec> {
  private currentSpec?: WidgetSpec;
  private static readonly TEMPLATE_DIR = path.join(__dirname, 'templates', 'widget');

  protected getModuleType(): string {
    return 'widget';
  }

  protected getModuleName(spec: WidgetSpec): string {
    return spec.name;
  }

  protected getOutputDir(spec: WidgetSpec): string {
    const kebabName = this.toKebabCase(spec.name);
    return path.join(this.rootPath, 'widgets', kebabName);
  }

  /**
   * Convert PascalCase to kebab-case
   */
  private toKebabCase(name: string): string {
    return name
      .replace(/([A-Z])/g, (match, p1, offset) =>
        offset > 0 ? '-' + p1.toLowerCase() : p1.toLowerCase()
      );
  }

  /**
   * Get the widget tag name (kebab-case-widget)
   */
  private getTagName(spec: WidgetSpec): string {
    return this.toKebabCase(spec.name) + '-widget';
  }

  /**
   * Build token map for template replacement
   */
  private buildTokens(spec: WidgetSpec): Record<string, string> {
    const kebabName = this.toKebabCase(spec.name);
    const tagName = this.getTagName(spec);

    const rightPanelJson = spec.rightPanel === null
      ? 'null'
      : spec.rightPanel
        ? JSON.stringify({ widgets: ['chat-widget'], config: spec.rightPanel }, null, 4)
            .split('\n').map((line, i) => i === 0 ? line : '    ' + line).join('\n')
        : '{ "widgets": ["chat-widget"], "config": { "room": "help", "compact": true } }';

    return {
      'WIDGET_NAME': spec.name,
      'KEBAB_NAME': kebabName,
      'TAG_NAME': tagName,
      'DESCRIPTION': spec.description,
      'DISPLAY_NAME': spec.displayName,
      'DISPLAY_NAME_LOWER': spec.displayName.toLowerCase(),
      'PATH_PREFIX': spec.pathPrefix,
      'REQUIRES_ENTITY': spec.requiresEntity ? 'Yes' : 'No',
      'RIGHT_PANEL_JSON': rightPanelJson,
      'PLACEHOLDER_TITLE': spec.placeholderTitle || spec.displayName,
      'PLACEHOLDER_TEXT': spec.placeholderText || 'Widget content goes here.',
    };
  }

  /**
   * Load and render a template file
   */
  private loadTemplate(templateName: string, tokens: Record<string, string>): string {
    const templatePath = path.join(WidgetGenerator.TEMPLATE_DIR, templateName);

    if (!fs.existsSync(templatePath)) {
      throw new Error(`Template not found: ${templateName} (${templatePath})`);
    }

    const template = fs.readFileSync(templatePath, 'utf-8');
    return TokenReplacer.replace(template, tokens);
  }

  protected renderTemplates(spec: WidgetSpec): Record<string, string> {
    this.currentSpec = spec;
    const tokens = this.buildTokens(spec);

    return {
      widget: this.loadTemplate('widget.template.ts', tokens),
      styles: this.loadTemplate('widget.template.scss', tokens),
      html: this.loadTemplate('widget.template.html', tokens),
      recipe: this.loadTemplate('recipe.template.json', tokens),
      readme: this.loadTemplate('README.template.md', tokens),
    };
  }

  protected writeFiles(baseDir: string, rendered: Record<string, string>): void {
    if (!this.currentSpec) {
      throw new Error('currentSpec not set');
    }

    const className = this.currentSpec.name + 'Widget';
    const kebabName = this.toKebabCase(this.currentSpec.name);
    const publicDir = path.join(baseDir, 'public');

    // Write widget TypeScript file
    const widgetPath = path.join(baseDir, `${className}.ts`);
    this.writeFile(widgetPath, rendered.widget);

    // Write HTML template file (in public/ subdirectory)
    const htmlPath = path.join(publicDir, `${kebabName}-widget.html`);
    this.writeFile(htmlPath, rendered.html);

    // Write SCSS styles file (in public/ subdirectory)
    const stylesPath = path.join(publicDir, `${kebabName}-widget.scss`);
    this.writeFile(stylesPath, rendered.styles);

    // Write README file
    const readmePath = path.join(baseDir, 'README.md');
    this.writeFile(readmePath, rendered.readme);

    // Write recipe JSON file
    const recipePath = path.join(this.rootPath, 'system', 'recipes', `${kebabName}.json`);
    this.writeFile(recipePath, rendered.recipe);

    // Print next steps
    console.log(`\n📋 Next steps:`);
    console.log(`   1. Compile SCSS: npx tsx scripts/compile-sass.ts`);
    console.log(`   2. Build: npm run build:ts`);
    console.log(`   3. Deploy: npm start`);
    console.log(`   4. Test: ./jtag interface/navigate --path="${this.currentSpec.pathPrefix}"`);
    console.log(`   5. Screenshot: ./jtag interface/screenshot`);
    console.log(`\n📚 Documentation:`);
    console.log(`   - README: ${readmePath}`);
    console.log(`   - Recipe: ${recipePath}`);
    console.log(`\n⚠️  If widget doesn't appear:`);
    console.log(`   - Check browser console for registration errors`);
    console.log(`   - Verify CSS compiled: ls ${publicDir}/${kebabName}-widget.css`);
    console.log(`   - Verify recipe seeded: ./jtag data/list --collection=recipes --filter='{"uniqueId":"${kebabName}"}'`);

    this.currentSpec = undefined;
  }
}


// CLI entry point removed — was causing esbuild to execute readFileSync at bundle time.
// Run generators via: npx tsx generator/<name>.ts
