#!/usr/bin/env tsx
/**
 * Module Compliance Report - Comprehensive status of all Continuum modules
 * 
 * Generates detailed compliance reports for all module types:
 * - Daemons, Widgets, Commands, Integrations
 * - Supports whitelisting for git hook integration
 * - Provides actionable compliance metrics
 */

import { IntelligentModularTestRunner } from './IntelligentModularTestRunner';

interface ComplianceWhitelist {
  daemons: {
    minimumCompliance: number; // Percentage (0-100)
    allowedNonCompliant: string[]; // Module names that are temporarily allowed to be non-compliant
  };
  widgets: {
    minimumCompliance: number;
    allowedNonCompliant: string[];
  };
  commands: {
    minimumCompliance: number;
    allowedNonCompliant: string[];
  };
  integrations: {
    minimumCompliance: number;
    allowedNonCompliant: string[];
  };
}

interface ComplianceStatus {
  type: string;
  totalModules: number;
  compliantModules: number;
  complianceRate: number;
  nonCompliantModules: string[];
  allowedNonCompliant: string[];
  unexpectedNonCompliant: string[];
  status: 'PASS' | 'FAIL' | 'WARNING';
}

interface OverallComplianceReport {
  timestamp: Date;
  moduleTypes: ComplianceStatus[];
  summary: {
    totalModules: number;
    totalCompliant: number;
    overallComplianceRate: number;
    passedTypes: number;
    failedTypes: number;
    status: 'PASS' | 'FAIL' | 'WARNING';
  };
}

class ModuleComplianceReport {
  private runner: IntelligentModularTestRunner;
  private whitelist: ComplianceWhitelist;

  constructor() {
    this.runner = new IntelligentModularTestRunner();
    
    // Default whitelist configuration
    this.whitelist = {
      daemons: {
        minimumCompliance: 95, // Very high standard for daemons
        allowedNonCompliant: [] // No exceptions - daemons must be compliant
      },
      widgets: {
        minimumCompliance: 70, // Moderate standard for widgets
        allowedNonCompliant: [
          'ActiveProjects',   // Needs main file + continuum.type (graduation candidate)
          'SavedPersonas',    // Needs main file + continuum.type (graduation candidate)
          'UserSelector',     // Needs main file + continuum.type (graduation candidate)
          'ChatRoom',         // Legacy widget being replaced
          'VersionWidget',    // Duplicate of Version widget  
          'commands',         // Not a real widget
          'domain',           // Not a real widget
          'intermediate',     // Not a real widget  
          'ui'                // Not a real widget
        ]
      },
      commands: {
        minimumCompliance: 50, // Low standard initially - lots of work needed
        allowedNonCompliant: [
          // All commands currently non-compliant - working on this
          'academy', 'ai', 'browser', 'communication', 'database',
          'development', 'devtools', 'docs', 'events', 'file',
          'input', 'kernel', 'monitoring', 'persona', 'planning',
          'system', 'testing', 'ui'
        ]
      },
      integrations: {
        minimumCompliance: 60, // Moderate standard for integrations
        allowedNonCompliant: [
          'websocket',  // Needs main file + continuum.type (graduation candidate)
          'academy',    // Legacy integration
          'devtools'    // Development-only integration
        ]
      }
    };
  }

  /**
   * Generate comprehensive compliance report for all module types
   */
  async generateReport(options: {
    includeDetails?: boolean;
    exitOnFailure?: boolean;
    useWhitelist?: boolean;
  } = {}): Promise<OverallComplianceReport> {
    const startTime = Date.now();
    
    console.log('🔍 COMPREHENSIVE MODULE COMPLIANCE REPORT');
    console.log('==========================================');
    console.log(`⏰ Generated: ${new Date().toISOString()}`);
    console.log('');

    const moduleTypes: ('daemon' | 'widget' | 'command' | 'integration')[] = 
      ['daemon', 'widget', 'command', 'integration'];

    const moduleStatuses: ComplianceStatus[] = [];
    let totalModules = 0;
    let totalCompliant = 0;

    for (const type of moduleTypes) {
      console.log(`📊 ${type.toUpperCase()} MODULES:`);
      console.log('-'.repeat(30));

      try {
        const results = await this.runner.runModuleTests(type);
        const whitelistConfig = this.whitelist[`${type}s` as keyof ComplianceWhitelist] as any;
        
        const nonCompliantNames = results.nonCompliantModules.map(m => m.name);
        const allowedNonCompliant = options.useWhitelist ? whitelistConfig.allowedNonCompliant : [];
        const unexpectedNonCompliant = nonCompliantNames.filter(name => 
          !allowedNonCompliant.includes(name)
        );

        const status = this.determineStatus(
          results.summary.complianceRate,
          whitelistConfig.minimumCompliance,
          unexpectedNonCompliant.length,
          options.useWhitelist ?? false
        );

        const moduleStatus: ComplianceStatus = {
          type: type.toUpperCase(),
          totalModules: results.summary.totalModules,
          compliantModules: results.summary.compliantCount,
          complianceRate: results.summary.complianceRate,
          nonCompliantModules: nonCompliantNames,
          allowedNonCompliant,
          unexpectedNonCompliant,
          status
        };

        moduleStatuses.push(moduleStatus);
        totalModules += results.summary.totalModules;
        totalCompliant += results.summary.compliantCount;

        // Print status
        const statusIcon = status === 'PASS' ? '✅' : status === 'WARNING' ? '⚠️' : '❌';
        console.log(`${statusIcon} ${moduleStatus.complianceRate.toFixed(1)}% compliant (${moduleStatus.compliantModules}/${moduleStatus.totalModules})`);
        
        if (options.useWhitelist && allowedNonCompliant.length > 0) {
          console.log(`   📋 Whitelisted non-compliant: ${allowedNonCompliant.length}`);
        }
        
        if (unexpectedNonCompliant.length > 0) {
          console.log(`   🚨 Unexpected non-compliant: ${unexpectedNonCompliant.join(', ')}`);
        }

        if (options.includeDetails && results.nonCompliantModules.length > 0) {
          console.log(`   📝 Non-compliant modules:`);
          for (const module of results.nonCompliantModules) {
            const isWhitelisted = allowedNonCompliant.includes(module.name);
            const icon = isWhitelisted ? '📋' : '🔴';
            console.log(`      ${icon} ${module.name} (${module.compliance.score}%)`);
            if (!isWhitelisted) {
              for (const issue of module.compliance.issues.slice(0, 2)) {
                console.log(`         → ${issue}`);
              }
            }
          }
        }

      } catch (error) {
        console.error(`❌ Failed to test ${type} modules:`, error);
        moduleStatuses.push({
          type: type.toUpperCase(),
          totalModules: 0,
          compliantModules: 0,
          complianceRate: 0,
          nonCompliantModules: [],
          allowedNonCompliant: [],
          unexpectedNonCompliant: [],
          status: 'FAIL'
        });
      }

      console.log('');
    }

    const overallComplianceRate = totalModules > 0 ? (totalCompliant / totalModules) * 100 : 0;
    const passedTypes = moduleStatuses.filter(s => s.status === 'PASS').length;
    const failedTypes = moduleStatuses.filter(s => s.status === 'FAIL').length;
    const overallStatus = failedTypes > 0 ? 'FAIL' : 
                         moduleStatuses.some(s => s.status === 'WARNING') ? 'WARNING' : 'PASS';

    const report: OverallComplianceReport = {
      timestamp: new Date(),
      moduleTypes: moduleStatuses,
      summary: {
        totalModules,
        totalCompliant,
        overallComplianceRate,
        passedTypes,
        failedTypes,
        status: overallStatus
      }
    };

    this.printSummary(report, Date.now() - startTime);

    if (options.exitOnFailure && report.summary.status === 'FAIL') {
      process.exit(1);
    }

    return report;
  }

  /**
   * Determine module type status based on compliance and whitelist
   */
  private determineStatus(
    complianceRate: number,
    minimumCompliance: number,
    unexpectedNonCompliantCount: number,
    useWhitelist: boolean
  ): 'PASS' | 'FAIL' | 'WARNING' {
    if (useWhitelist) {
      if (unexpectedNonCompliantCount === 0) {
        return 'PASS';
      } else if (complianceRate >= minimumCompliance * 0.8) {
        return 'WARNING';
      } else {
        return 'FAIL';
      }
    } else {
      if (complianceRate >= minimumCompliance) {
        return 'PASS';
      } else if (complianceRate >= minimumCompliance * 0.8) {
        return 'WARNING';
      } else {
        return 'FAIL';
      }
    }
  }

  /**
   * Print comprehensive summary
   */
  private printSummary(report: OverallComplianceReport, duration: number): void {
    console.log('📈 OVERALL COMPLIANCE SUMMARY');
    console.log('=============================');
    
    const statusIcon = report.summary.status === 'PASS' ? '✅' : 
                      report.summary.status === 'WARNING' ? '⚠️' : '❌';
    
    console.log(`${statusIcon} Overall Status: ${report.summary.status}`);
    console.log(`📦 Total Modules: ${report.summary.totalModules}`);
    console.log(`✅ Compliant: ${report.summary.totalCompliant} (${report.summary.overallComplianceRate.toFixed(1)}%)`);
    console.log(`🎯 Module Types: ${report.summary.passedTypes}/${report.moduleTypes.length} passing`);
    console.log(`⏱️ Duration: ${duration}ms`);
    console.log('');

    // Module type breakdown
    console.log('📊 Module Type Breakdown:');
    for (const moduleType of report.moduleTypes) {
      const icon = moduleType.status === 'PASS' ? '✅' : 
                   moduleType.status === 'WARNING' ? '⚠️' : '❌';
      console.log(`   ${icon} ${moduleType.type}: ${moduleType.complianceRate.toFixed(1)}% (${moduleType.compliantModules}/${moduleType.totalModules})`);
    }

    if (report.summary.status === 'FAIL') {
      console.log('');
      console.log('🚨 COMPLIANCE FAILURES DETECTED!');
      console.log('   Fix non-compliant modules or update whitelist before proceeding.');
    } else if (report.summary.status === 'WARNING') {
      console.log('');
      console.log('⚠️ COMPLIANCE WARNINGS DETECTED!');
      console.log('   Consider addressing issues or updating whitelist.');
    } else {
      console.log('');
      console.log('🎉 ALL MODULE TYPES COMPLIANT!');
      console.log('   Excellent modular architecture adherence!');
    }
  }

  /**
   * Load custom whitelist configuration
   */
  loadWhitelist(configPath: string): void {
    try {
      const fs = require('fs');
      const customWhitelist = JSON.parse(fs.readFileSync(configPath, 'utf8'));
      this.whitelist = { ...this.whitelist, ...customWhitelist };
      console.log(`📋 Loaded custom whitelist from ${configPath}`);
    } catch (error) {
      console.warn(`⚠️ Failed to load whitelist from ${configPath}:`, error);
    }
  }
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const options = {
    includeDetails: args.includes('--details'),
    exitOnFailure: args.includes('--exit-on-failure'),
    useWhitelist: args.includes('--use-whitelist')
  };

  const reporter = new ModuleComplianceReport();
  
  // Load custom whitelist if provided
  const whitelistIndex = args.findIndex(arg => arg === '--whitelist');
  if (whitelistIndex !== -1 && args[whitelistIndex + 1]) {
    reporter.loadWhitelist(args[whitelistIndex + 1]);
  }

  reporter.generateReport(options).catch(error => {
    console.error('❌ Report generation failed:', error);
    process.exit(1);
  });
}

export { ModuleComplianceReport };
export type { ComplianceWhitelist, OverallComplianceReport };