/**
 * WidgetGeneratorType — IGeneratorType implementation for widgets
 *
 * Provides audit, fix, reverse-engineer, and help for widget modules.
 * Widgets have: TypeScript class, HTML template, SCSS styles, recipe, README.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { WidgetSpec } from '../WidgetGenerator';
import type {
  IGeneratorType,
  GeneratorAuditSummary,
  GeneratorAuditEntry,
  FixSummary,
  FixResult,
  GeneratorHelp
} from '../GeneratorSDK';
import type { GenerateOptions } from '../ModuleGenerator';
import { WidgetGenerator } from '../WidgetGenerator';

export class WidgetGeneratorType implements IGeneratorType<WidgetSpec> {
  readonly typeName = 'widget';
  readonly description = 'UI widgets with Lit, HTML templates, SCSS styles, and recipes';

  private readonly rootPath: string;
  private readonly generator: WidgetGenerator;
  private readonly widgetsDir: string;
  private readonly specsDir: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
    this.generator = new WidgetGenerator(rootPath);
    this.widgetsDir = path.join(rootPath, 'widgets');
    this.specsDir = path.join(rootPath, 'generator', 'specs', 'widgets');
  }

  // ── Generate ────────────────────────────────────────────────────

  generate(spec: WidgetSpec, outputDir?: string, options?: GenerateOptions): void {
    this.generator.generate(spec, outputDir, options);
  }

  generateFromFile(specFilePath: string, outputDir?: string, options?: GenerateOptions): void {
    const spec: WidgetSpec = JSON.parse(fs.readFileSync(specFilePath, 'utf-8'));
    this.generator.generate(spec, outputDir, options);
  }

  // ── Audit ───────────────────────────────────────────────────────

  audit(): GeneratorAuditSummary {
    const widgetDirs = this.discoverWidgets();
    const specMap = this.loadSpecMap();
    const entries: GeneratorAuditEntry[] = [];

    const checkSummary: Record<string, { passing: number; failing: number }> = {
      'has-spec': { passing: 0, failing: 0 },
      'has-widget-class': { passing: 0, failing: 0 },
      'has-html-template': { passing: 0, failing: 0 },
      'has-styles': { passing: 0, failing: 0 },
      'has-recipe': { passing: 0, failing: 0 },
      'has-readme': { passing: 0, failing: 0 },
      'extends-base-widget': { passing: 0, failing: 0 },
    };

    for (const { name, dirPath } of widgetDirs) {
      const hasSpec = name in specMap;
      const widgetFile = this.findWidgetFile(dirPath);
      const widgetContent = widgetFile ? fs.readFileSync(widgetFile, 'utf-8') : '';

      const kebabName = name;
      const publicDir = path.join(dirPath, 'public');

      const checks: Record<string, boolean> = {
        'has-spec': hasSpec,
        'has-widget-class': !!widgetFile,
        'has-html-template': fs.existsSync(path.join(publicDir, `${kebabName}-widget.html`)) ||
          fs.existsSync(path.join(publicDir, `${kebabName}.html`)),
        'has-styles': fs.existsSync(path.join(publicDir, `${kebabName}-widget.scss`)) ||
          fs.existsSync(path.join(publicDir, `${kebabName}-widget.css`)) ||
          fs.existsSync(path.join(publicDir, `${kebabName}.scss`)),
        'has-recipe': this.hasRecipe(kebabName),
        'has-readme': fs.existsSync(path.join(dirPath, 'README.md')),
        'extends-base-widget': /extends\s+BaseWidget/.test(widgetContent),
      };

      const issues: string[] = [];
      const fixableIssues: string[] = [];

      if (!checks['has-spec']) issues.push('No matching widget spec');
      if (!checks['has-widget-class']) issues.push('Missing widget TypeScript class');
      if (!checks['has-html-template']) issues.push('Missing HTML template');
      if (!checks['has-styles']) issues.push('Missing styles (SCSS/CSS)');
      if (!checks['has-recipe']) issues.push('Missing recipe JSON');
      if (!checks['has-readme']) {
        issues.push('Missing README');
        if (hasSpec) fixableIssues.push('Missing README (can generate from spec)');
      }
      if (!checks['extends-base-widget'] && widgetContent) issues.push('Does not extend BaseWidget');

      for (const [checkName, passing] of Object.entries(checks)) {
        if (passing) checkSummary[checkName].passing++;
        else checkSummary[checkName].failing++;
      }

      entries.push({
        name,
        path: dirPath,
        hasSpec,
        specPath: hasSpec ? specMap[name] : undefined,
        checks,
        issues,
        fixableIssues,
      });
    }

    const widgetNames = new Set(widgetDirs.map(w => w.name));
    const orphanedSpecs = Object.keys(specMap).filter(name => !widgetNames.has(name));

    return {
      type: this.typeName,
      entries,
      total: widgetDirs.length,
      withSpecs: entries.filter(e => e.hasSpec).length,
      totalIssues: entries.reduce((sum, e) => sum + e.issues.length, 0),
      totalFixable: entries.reduce((sum, e) => sum + e.fixableIssues.length, 0),
      orphanedSpecs,
      checkSummary,
    };
  }

  auditOne(modulePath: string): GeneratorAuditEntry {
    const summary = this.audit();
    const entry = summary.entries.find(e =>
      e.path === modulePath || e.name === modulePath
    );
    if (!entry) {
      return {
        name: modulePath, path: modulePath, hasSpec: false,
        checks: {}, issues: [`Widget not found: ${modulePath}`], fixableIssues: [],
      };
    }
    return entry;
  }

  // ── Fix ─────────────────────────────────────────────────────────

  fixAll(): FixSummary {
    return { type: this.typeName, results: [], totalFixed: 0, totalRemaining: 0 };
  }

  fixOne(_modulePath: string): FixResult {
    return {
      name: _modulePath, filesModified: [], filesCreated: [],
      issuesFixed: [],
      issuesRemaining: ['Widget auto-fix not yet implemented — widgets have complex UI dependencies'],
    };
  }

  // ── Reverse Engineer ────────────────────────────────────────────

  reverseEngineer(modulePath: string): WidgetSpec | null {
    const absDir = path.isAbsolute(modulePath) ? modulePath :
      path.join(this.widgetsDir, modulePath);

    const widgetFile = this.findWidgetFile(absDir);
    if (!widgetFile) return null;
    const content = fs.readFileSync(widgetFile, 'utf-8');

    // Extract name
    const classMatch = content.match(/export\s+class\s+(\w+)Widget/);
    const name = classMatch?.[1] || path.basename(absDir);

    // Extract description
    const descMatch = content.match(/\*\s+(.+?)\s*\n/);
    const description = descMatch?.[1]?.replace(/Widget\s*-?\s*/i, '').trim() || 'TODO';

    // Extract tag name
    const tagMatch = content.match(/customElements\.define\s*\(\s*'([^']+)'/);
    const displayName = name.replace(/([A-Z])/g, ' $1').trim();

    // Extract path prefix from routing
    const pathMatch = content.match(/pathPrefix.*?['"]([^'"]+)/);
    const pathPrefix = pathMatch?.[1] || `/${name.toLowerCase()}`;

    return {
      name,
      description,
      displayName,
      pathPrefix,
      requiresEntity: content.includes('entityId'),
    };
  }

  // ── Help & Templates ────────────────────────────────────────────

  help(): GeneratorHelp {
    const full = `
WIDGET GENERATOR
================

Generates UI widget modules with Lit elements, HTML templates, SCSS styles, and recipes.

SPEC FORMAT (WidgetSpec):
  {
    "name": "UserProfile",                    // PascalCase, no 'Widget' suffix
    "description": "Displays user profile",
    "displayName": "Profile",
    "pathPrefix": "/profile",
    "requiresEntity": true,
    "rightPanel": { "room": "help", "compact": true },
    "placeholderTitle": "User Profile",
    "placeholderText": "Select a user to view their profile."
  }

GENERATED OUTPUT:
  widgets/{kebab-name}/
    {Name}Widget.ts                - Widget class extending BaseWidget
    public/{kebab-name}-widget.html  - HTML template
    public/{kebab-name}-widget.scss  - SCSS styles
    README.md                      - Documentation
  system/recipes/{kebab-name}.json - Layout recipe

WORKFLOW:
  1. Create spec JSON
  2. Generate: npx tsx generator/WidgetGenerator.ts <spec.json>
  3. Compile SCSS: npx tsx scripts/compile-sass.ts
  4. Build: npm run build:ts
  5. Deploy: npm start
`;

    return {
      full,
      short: 'Usage: npx tsx generator/WidgetGenerator.ts <spec.json>',
      topics: { spec: full },
      availableTopics: ['spec'],
    };
  }

  templateSpec(_variant: string = 'standard'): WidgetSpec {
    return {
      name: 'Example',
      description: 'Example widget',
      displayName: 'Example',
      pathPrefix: '/example',
      requiresEntity: false,
      rightPanel: { room: 'help', compact: true },
      placeholderTitle: 'Example Widget',
      placeholderText: 'This is placeholder content.',
    };
  }

  templateVariants(): string[] {
    return ['standard'];
  }

  // ── Private Helpers ─────────────────────────────────────────────

  private discoverWidgets(): Array<{ name: string; dirPath: string }> {
    const results: Array<{ name: string; dirPath: string }> = [];
    if (!fs.existsSync(this.widgetsDir)) return results;

    const entries = fs.readdirSync(this.widgetsDir, { withFileTypes: true });
    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      if (entry.name === 'node_modules' || entry.name === 'shared') continue;

      const dirPath = path.join(this.widgetsDir, entry.name);
      // A widget dir has a .ts file or a public/ directory
      const hasTs = fs.readdirSync(dirPath).some(f => f.endsWith('Widget.ts'));
      const hasPublic = fs.existsSync(path.join(dirPath, 'public'));
      if (hasTs || hasPublic) {
        results.push({ name: entry.name, dirPath });
      }
    }

    return results;
  }

  private loadSpecMap(): Record<string, string> {
    const map: Record<string, string> = {};
    if (!fs.existsSync(this.specsDir)) return map;

    const files = fs.readdirSync(this.specsDir).filter(f => f.endsWith('.json'));
    for (const file of files) {
      const fullPath = path.join(this.specsDir, file);
      try {
        const spec: WidgetSpec = JSON.parse(fs.readFileSync(fullPath, 'utf-8'));
        // Widget spec name is PascalCase, convert to kebab for matching
        const kebab = spec.name.replace(/([A-Z])/g, (m, p, o) =>
          o > 0 ? '-' + p.toLowerCase() : p.toLowerCase()
        );
        map[kebab] = fullPath;
      } catch { /* skip */ }
    }

    return map;
  }

  private findWidgetFile(dirPath: string): string | null {
    if (!fs.existsSync(dirPath)) return null;
    const files = fs.readdirSync(dirPath).filter(f => f.endsWith('Widget.ts'));
    return files[0] ? path.join(dirPath, files[0]) : null;
  }

  private hasRecipe(kebabName: string): boolean {
    const recipesDir = path.join(this.rootPath, 'system', 'recipes');
    if (!fs.existsSync(recipesDir)) return false;
    return fs.existsSync(path.join(recipesDir, `${kebabName}.json`));
  }
}
