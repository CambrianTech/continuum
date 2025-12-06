/**
 * TemplateLoader - Loads template files from disk and renders them with token replacement
 *
 * Provides utilities to load external template files and render them with TokenReplacer.
 */

import * as fs from 'fs';
import * as path from 'path';
import { TokenReplacer } from './TokenReplacer';
import { TokenBuilder } from './TokenBuilder';
import type { CommandSpec } from './CommandNaming';

export class TemplateLoader {
  private static readonly TEMPLATE_DIR = path.join(__dirname, 'templates');

  /**
   * Load a template file from disk
   *
   * @param templatePath - Relative path from templates/ directory (e.g., 'command/shared-types.template.ts')
   * @returns Template content as string
   */
  static loadTemplate(templatePath: string): string {
    const fullPath = path.join(this.TEMPLATE_DIR, templatePath);

    if (!fs.existsSync(fullPath)) {
      throw new Error(`Template not found: ${templatePath} (${fullPath})`);
    }

    return fs.readFileSync(fullPath, 'utf-8');
  }

  /**
   * Render a template with provided tokens
   *
   * @param templatePath - Relative path from templates/ directory
   * @param tokens - Token values to replace in template
   * @returns Rendered template content
   */
  static renderTemplate(templatePath: string, tokens: Record<string, string>): string {
    const template = this.loadTemplate(templatePath);

    // Validate that all required tokens are present
    const missing = TokenReplacer.validateTokens(template, tokens);
    if (missing.length > 0) {
      throw new Error(`Missing required tokens: ${missing.join(', ')}`);
    }

    return TokenReplacer.replace(template, tokens);
  }

  /**
   * Render command templates from a CommandSpec
   *
   * @param spec - Command specification
   * @returns Object with rendered templates for each file
   */
  static renderCommand(spec: CommandSpec): {
    sharedTypes: string;
    browser: string;
    server: string;
    readme: string;
    unitTest: string;
    integrationTest: string;
    tokens: Record<string, string>;
  } {
    const tokens = TokenBuilder.buildAllTokens(spec);

    return {
      sharedTypes: this.renderTemplate('command/shared-types.template.ts', tokens),
      browser: this.renderTemplate('command/browser.template.ts', tokens),
      server: this.renderTemplate('command/server.template.ts', tokens),
      readme: this.renderTemplate('command/README.template.md', tokens),
      unitTest: this.renderTemplate('command/unit-test.template.ts', tokens),
      integrationTest: this.renderTemplate('command/integration-test.template.ts', tokens),
      tokens // Return tokens for debugging
    };
  }

  /**
   * List all available templates
   *
   * @returns Array of template paths relative to templates/ directory
   */
  static listTemplates(): string[] {
    const templates: string[] = [];

    const walkDir = (dir: string, prefix = ''): void => {
      const files = fs.readdirSync(dir);

      for (const file of files) {
        const fullPath = path.join(dir, file);
        const stat = fs.statSync(fullPath);

        if (stat.isDirectory()) {
          walkDir(fullPath, path.join(prefix, file));
        } else if (file.endsWith('.template.ts') || file.endsWith('.template.md')) {
          templates.push(path.join(prefix, file));
        }
      }
    };

    if (fs.existsSync(this.TEMPLATE_DIR)) {
      walkDir(this.TEMPLATE_DIR);
    }

    return templates;
  }

  /**
   * Write rendered template to file system
   *
   * @param outputPath - Full path to output file
   * @param content - Rendered template content
   */
  static writeToFile(outputPath: string, content: string): void {
    const dir = path.dirname(outputPath);

    // Create directory if it doesn't exist
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    fs.writeFileSync(outputPath, content, 'utf-8');
  }
}
