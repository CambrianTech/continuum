/**
 * Generate/Audit Server Command
 *
 * Audits generated modules for issues and optionally fixes them
 */

import * as fs from 'fs';
import * as path from 'path';
import type { GenerateAuditParams, GenerateAuditResult } from '../shared/GenerateAuditTypes';
import { createGenerateAuditResultFromParams } from '../shared/GenerateAuditTypes';
import { ModuleAuditor } from '@generator/audit/ModuleAuditor';
import { LintCheck } from '@generator/audit/checks/LintCheck';
import { MissingFileCheck } from '@generator/audit/checks/MissingFileCheck';
import { OutdatedPatternCheck } from '@generator/audit/checks/OutdatedPatternCheck';
import { PackageJsonCheck } from '@generator/audit/checks/PackageJsonCheck';
import { ReadmeCheck } from '@generator/audit/checks/ReadmeCheck';
import { TestCoverageCheck } from '@generator/audit/checks/TestCoverageCheck';
import type { AuditReport, ModuleType } from '@generator/audit/AuditTypes';

export class GenerateAuditServerCommand {
  /**
   * Execute the audit command
   */
  static async execute(params: GenerateAuditParams): Promise<GenerateAuditResult> {
    try {
      // Validate parameters
      if (!params.module && !params.type) {
        return createGenerateAuditResultFromParams(params, {
          success: false,
          error: 'Must specify either --module or --type',
        });
      }

      // Create auditor and register checks
      const auditor = new ModuleAuditor();
      auditor.registerCheck(new LintCheck());
      auditor.registerCheck(new MissingFileCheck());
      auditor.registerCheck(new OutdatedPatternCheck());
      auditor.registerCheck(new PackageJsonCheck());
      auditor.registerCheck(new ReadmeCheck());
      auditor.registerCheck(new TestCoverageCheck());

      const reports: AuditReport[] = [];

      // Audit specific module
      if (params.module) {
        const report = await this.auditModule(
          auditor,
          params.module,
          this.detectModuleType(params.module),
          params
        );
        reports.push(report);
      }

      // Audit all modules of type
      if (params.type) {
        const modules = this.findModulesOfType(params.type);
        for (const modulePath of modules) {
          const report = await this.auditModule(
            auditor,
            modulePath,
            params.type,
            params
          );
          reports.push(report);
        }
      }

      // Calculate summary
      const summary = {
        modulesAudited: reports.length,
        totalErrors: reports.reduce((sum, r) => sum + r.summary.errors, 0),
        totalWarnings: reports.reduce((sum, r) => sum + r.summary.warnings, 0),
        totalFixed: reports.reduce((sum, r) => sum + r.summary.fixable, 0),
      };

      return createGenerateAuditResultFromParams(params, {
        success: true,
        reports,
        summary,
      });
    } catch (error) {
      return createGenerateAuditResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /**
   * Audit a single module
   */
  private static async auditModule(
    auditor: ModuleAuditor,
    modulePath: string,
    moduleType: ModuleType,
    params: GenerateAuditParams
  ): Promise<AuditReport> {
    console.log(`\n🔍 Auditing ${modulePath}...`);

    const report = await auditor.audit(modulePath, moduleType, {
      fix: params.fix,
      hibernateFailures: params.hibernateFailures,
    });

    // Display report
    console.log(auditor.formatReport(report));

    return report;
  }

  /**
   * Detect module type from path
   */
  private static detectModuleType(modulePath: string): ModuleType {
    if (modulePath.startsWith('commands/')) return 'command';
    if (modulePath.startsWith('widgets/')) return 'widget';
    if (modulePath.startsWith('daemons/')) return 'daemon';
    return 'command'; // Default
  }

  /**
   * Find all modules of a given type (recursively)
   */
  private static findModulesOfType(type: ModuleType): string[] {
    const baseDir = type === 'command' ? 'commands' : type === 'widget' ? 'widgets' : 'daemons';
    const modules: string[] = [];

    if (!fs.existsSync(baseDir)) {
      return modules;
    }

    this.findModulesRecursive(baseDir, modules);

    return modules;
  }

  /**
   * Recursively find modules with shared/ directory
   */
  private static findModulesRecursive(dir: string, modules: string[]): void {
    const entries = fs.readdirSync(dir, { withFileTypes: true });

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      // Skip backup directories (hibernation pollution check would catch these)
      if (entry.name.includes('.backup.')) continue;

      const fullPath = path.join(dir, entry.name);

      // If this directory has shared/, it's a module
      if (fs.existsSync(path.join(fullPath, 'shared'))) {
        modules.push(fullPath);
      } else {
        // Otherwise, recurse into it to find nested modules
        this.findModulesRecursive(fullPath, modules);
      }
    }
  }
}
