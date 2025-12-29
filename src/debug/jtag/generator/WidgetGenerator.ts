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

// CLI execution
if (require.main === module) {
  const args = process.argv.slice(2);

  if (args.length === 0) {
    console.error('Usage: npx tsx generator/WidgetGenerator.ts <spec-file.json> [--force] [--backup]');
    console.error('   or: npx tsx generator/WidgetGenerator.ts --template');
    console.error('   or: npx tsx generator/WidgetGenerator.ts --test');
    console.error('\nFlags:');
    console.error('  --force   Overwrite existing widget');
    console.error('  --backup  Create backup before overwriting');
    process.exit(1);
  }

  const rootPath = path.join(__dirname, '..');
  const generator = new WidgetGenerator(rootPath);

  if (args[0] === '--template') {
    const exampleSpec: WidgetSpec = {
      name: 'Example',
      description: 'Brief description of what this widget does',
      displayName: 'Example',
      pathPrefix: '/example',
      requiresEntity: false,
      rightPanel: { room: 'help', compact: true },
      placeholderTitle: 'Example Widget',
      placeholderText: 'This is placeholder content for the example widget.'
    };
    console.log(JSON.stringify(exampleSpec, null, 2));
    console.log('\n📝 Copy the JSON above, edit it, save to a file, then run:');
    console.log('   npx tsx generator/WidgetGenerator.ts <your-spec-file.json>');
    process.exit(0);
  } else if (args[0] === '--test') {
    const testSpec: WidgetSpec = {
      name: 'Test',
      description: 'Test widget for generator testing',
      displayName: 'Test',
      pathPrefix: '/test',
      requiresEntity: false,
      rightPanel: null,
      placeholderTitle: 'Test Widget',
      placeholderText: 'Generated by WidgetGenerator test mode.'
    };
    console.log('🧪 Test Mode: Generating test widget to /tmp...\n');
    generator.generate(testSpec, '/tmp/generated-widget-test');
  } else {
    // Normal mode: generate from spec file
    const specFile = args[0];
    const flagArgs = args.filter(a => a.startsWith('--'));

    const options: GenerateOptions = {
      force: flagArgs.includes('--force'),
      backup: flagArgs.includes('--backup')
    };

    if (options.backup && !options.force) {
      console.error('❌ ERROR: --backup requires --force');
      process.exit(1);
    }

    const specJson = fs.readFileSync(specFile, 'utf-8');
    const spec: WidgetSpec = JSON.parse(specJson);
    generator.generate(spec, undefined, options);
  }
}

export { WidgetSpec };
