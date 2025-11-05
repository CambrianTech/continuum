#!/usr/bin/env tsx
/**
 * Module Graduation Tracker - Gradual compliance progression
 * 
 * Tracks modules as they "graduate" from whitelist to full compliance:
 * - Shows graduation candidates (ready to remove from whitelist)
 * - Tracks graduation progress over time
 * - Provides celebration of achievements
 * - Suggests next graduation targets
 */

import { IntelligentModularTestRunner } from './IntelligentModularTestRunner';
import * as fs from 'fs';
import * as path from 'path';

interface GraduationCandidate {
  name: string;
  type: 'widget' | 'daemon' | 'command' | 'integration';
  complianceScore: number;
  isWhitelisted: boolean;
  canGraduate: boolean;
  blockers: string[];
  readySteps: string[];
}

interface GraduationRecord {
  timestamp: Date;
  module: string;
  type: string;
  fromScore: number;
  toScore: number;
  action: 'graduated' | 'whitelisted' | 'regressed';
}

class ModuleGraduationTracker {
  private runner: IntelligentModularTestRunner;
  private graduationFile: string;

  constructor() {
    this.runner = new IntelligentModularTestRunner();
    this.graduationFile = path.join(process.cwd(), '.continuum', 'graduation-history.json');
    this.ensureHistoryDirectory();
  }

  private ensureHistoryDirectory(): void {
    const dir = path.dirname(this.graduationFile);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
  }

  /**
   * Check for graduation candidates across all module types
   */
  async checkGraduationCandidates(): Promise<void> {
    console.log('🎓 MODULE GRADUATION TRACKER');
    console.log('============================');
    console.log('💡 Tracking modules ready to graduate from whitelist');
    console.log('');

    const whitelist = this.getWhitelist();
    const moduleTypes: ('daemon' | 'widget' | 'command' | 'integration')[] = 
      ['daemon', 'widget', 'command', 'integration'];

    let totalCandidates = 0;
    let readyToGraduate = 0;

    for (const type of moduleTypes) {
      console.log(`🎯 ${type.toUpperCase()} MODULES:`);
      console.log('-'.repeat(30));

      try {
        // Run tests silently
        const originalLog = console.log;
        console.log = () => {};
        const results = await this.runner.runModuleTests(type);
        console.log = originalLog;

        const typeWhitelist = whitelist[`${type}s` as keyof typeof whitelist]?.allowedNonCompliant || [];
        const candidates = this.analyzeCandidates(results, type, typeWhitelist);
        
        totalCandidates += candidates.length;
        const ready = candidates.filter(c => c.canGraduate);
        readyToGraduate += ready.length;

        if (candidates.length === 0) {
          console.log('   🎉 All modules either compliant or appropriately whitelisted!');
        } else {
          // Show graduation candidates
          for (const candidate of candidates) {
            if (candidate.canGraduate) {
              console.log(`   🎓 READY TO GRADUATE: ${candidate.name} (${candidate.complianceScore}%)`);
              console.log(`      📝 Remove from whitelist - fully compliant!`);
            } else if (candidate.isWhitelisted) {
              console.log(`   📋 Whitelisted: ${candidate.name} (${candidate.complianceScore}%)`);
              if (candidate.complianceScore >= 50) {
                console.log(`      🎯 Graduation candidate - only ${candidate.blockers.length} blocker(s):`);
                candidate.blockers.slice(0, 2).forEach(blocker => {
                  console.log(`         → ${blocker}`);
                });
              }
            } else {
              console.log(`   ❌ Non-compliant: ${candidate.name} (${candidate.complianceScore}%)`);
              console.log(`      💡 Consider whitelisting if intentionally non-compliant`);
            }
          }
        }

      } catch (error) {
        console.error(`❌ Failed to check ${type} candidates:`, error);
      }

      console.log('');
    }

    this.showGraduationSummary(totalCandidates, readyToGraduate);
    this.showGraduationSuggestions(readyToGraduate);
  }

  /**
   * Analyze modules to find graduation candidates
   */
  private analyzeCandidates(
    results: any, 
    type: string, 
    whitelist: string[]
  ): GraduationCandidate[] {
    const candidates: GraduationCandidate[] = [];

    // Check compliant modules that are whitelisted (ready to graduate)
    for (const module of results.compliantModules) {
      if (whitelist.includes(module.name)) {
        candidates.push({
          name: module.name,
          type: type as any,
          complianceScore: module.compliance.score,
          isWhitelisted: true,
          canGraduate: true,
          blockers: [],
          readySteps: ['Remove from whitelist', 'Update compliance configuration']
        });
      }
    }

    // Check non-compliant modules
    for (const module of results.nonCompliantModules) {
      const isWhitelisted = whitelist.includes(module.name);
      const canGraduate = module.compliance.score >= 70 && isWhitelisted;
      
      candidates.push({
        name: module.name,
        type: type as any,
        complianceScore: module.compliance.score,
        isWhitelisted,
        canGraduate,
        blockers: module.compliance.issues,
        readySteps: canGraduate ? 
          ['Fix remaining blockers', 'Remove from whitelist'] :
          ['Fix compliance issues']
      });
    }

    return candidates.sort((a, b) => {
      // Ready to graduate first, then by compliance score
      if (a.canGraduate && !b.canGraduate) return -1;
      if (!a.canGraduate && b.canGraduate) return 1;
      return b.complianceScore - a.complianceScore;
    });
  }

  /**
   * Show graduation summary and celebration
   */
  private showGraduationSummary(totalCandidates: number, readyToGraduate: number): void {
    console.log('🎓 GRADUATION SUMMARY');
    console.log('====================');
    
    if (readyToGraduate > 0) {
      console.log(`🎉 READY TO GRADUATE: ${readyToGraduate} modules!`);
      console.log(`   These modules have achieved full compliance while whitelisted`);
      console.log(`   Safe to remove from whitelist and celebrate! 🎊`);
    } else {
      console.log(`📚 In Progress: ${totalCandidates} modules working toward graduation`);
      console.log(`   Keep up the incremental improvements!`);
    }

    console.log('');
  }

  /**
   * Show actionable graduation suggestions
   */
  private showGraduationSuggestions(readyToGraduate: number): void {
    console.log('🎯 GRADUATION ACTION PLAN:');
    console.log('==========================');

    if (readyToGraduate > 0) {
      console.log('**IMMEDIATE ACTIONS:**');
      console.log('1. 🎓 Remove graduated modules from whitelist');
      console.log('   → Edit src/testing/ModuleComplianceReport.ts');
      console.log('   → Remove module names from allowedNonCompliant arrays');
      console.log('   → Commit with celebration message! 🎉');
      console.log('');
      console.log('2. ✅ Verify compliance after whitelist update');
      console.log('   → Run `npm run test:compliance`');
      console.log('   → Should see improved compliance rates');
      console.log('');
    }

    console.log('**ONGOING DEVELOPMENT:**');
    console.log('1. 📈 Focus on modules closest to graduation (>50% compliance)');
    console.log('2. 🎯 Fix one blocker at a time, commit incrementally');
    console.log('3. 📋 Add modules to whitelist if intentionally non-compliant');
    console.log('4. 🔄 Run graduation check regularly to track progress');
    console.log('');
    console.log('💡 **Pro Tip:** Small, focused commits keep git hook happy!');
    console.log('   Fix one compliance issue → commit → repeat');
  }

  /**
   * Record a graduation event
   */
  recordGraduation(module: string, type: string, fromScore: number, toScore: number): void {
    try {
      const history = this.loadGraduationHistory();
      const record: GraduationRecord = {
        timestamp: new Date(),
        module,
        type,
        fromScore,
        toScore,
        action: toScore >= 70 ? 'graduated' : 'whitelisted'
      };
      
      history.push(record);
      
      // Keep last 50 records
      if (history.length > 50) {
        history.splice(0, history.length - 50);
      }
      
      fs.writeFileSync(this.graduationFile, JSON.stringify(history, null, 2));
      
      if (record.action === 'graduated') {
        console.log(`🎉 GRADUATION RECORDED: ${module} (${type}) achieved ${toScore}% compliance!`);
      }
    } catch (error) {
      console.warn('⚠️ Failed to record graduation:', error);
    }
  }

  /**
   * Load graduation history
   */
  private loadGraduationHistory(): GraduationRecord[] {
    try {
      if (fs.existsSync(this.graduationFile)) {
        return JSON.parse(fs.readFileSync(this.graduationFile, 'utf8'));
      }
    } catch (error) {
      console.warn('⚠️ Failed to load graduation history:', error);
    }
    return [];
  }

  /**
   * Get current whitelist configuration
   */
  private getWhitelist() {
    // This mirrors the whitelist from ModuleComplianceReport.ts
    return {
      daemons: {
        allowedNonCompliant: []
      },
      widgets: {
        allowedNonCompliant: [
          'ChatRoom',      // Legacy widget being replaced
          'VersionWidget', // Duplicate of Version widget
          'commands',      // Not a real widget
          'domain',        // Not a real widget
          'intermediate',  // Not a real widget  
          'ui'             // Not a real widget
        ]
      },
      commands: {
        allowedNonCompliant: [
          'academy', 'ai', 'browser', 'communication', 'database',
          'development', 'devtools', 'docs', 'events', 'file',
          'input', 'kernel', 'monitoring', 'persona', 'planning',
          'system', 'testing', 'ui'
        ]
      },
      integrations: {
        allowedNonCompliant: [
          'academy',  // Legacy integration
          'devtools'  // Development-only integration
        ]
      }
    };
  }

  /**
   * Suggest next module to work on based on graduation potential
   */
  async suggestNextTarget(): Promise<void> {
    console.log('🎯 NEXT TARGET SUGGESTION');
    console.log('=========================');
    
    // Find the highest-scoring non-compliant whitelisted module
    const moduleTypes: ('daemon' | 'widget' | 'command' | 'integration')[] = 
      ['widget', 'integration', 'command', 'daemon']; // Order by graduation potential

    let bestCandidate: GraduationCandidate | null = null;

    for (const type of moduleTypes) {
      try {
        const originalLog = console.log;
        console.log = () => {};
        const results = await this.runner.runModuleTests(type);
        console.log = originalLog;

        const whitelist = this.getWhitelist()[`${type}s` as keyof ReturnType<typeof this.getWhitelist>]?.allowedNonCompliant || [];
        const candidates = this.analyzeCandidates(results, type, whitelist);
        
        const graduationCandidate = candidates
          .filter(c => c.isWhitelisted && !c.canGraduate && c.complianceScore >= 30)
          .sort((a, b) => b.complianceScore - a.complianceScore)[0];

        if (graduationCandidate && (!bestCandidate || graduationCandidate.complianceScore > bestCandidate.complianceScore)) {
          bestCandidate = graduationCandidate;
        }
      } catch (error) {
        // Skip this type
      }
    }

    if (bestCandidate) {
      console.log(`💡 RECOMMENDED NEXT TARGET: ${bestCandidate.name} (${bestCandidate.type})`);
      console.log(`   📊 Current compliance: ${bestCandidate.complianceScore}%`);
      console.log(`   🎯 Distance to graduation: ${70 - bestCandidate.complianceScore}% to go`);
      console.log(`   🔧 Remaining blockers: ${bestCandidate.blockers.length}`);
      console.log('');
      console.log('   Next steps:');
      bestCandidate.blockers.slice(0, 3).forEach((blocker, i) => {
        console.log(`   ${i + 1}. ${blocker}`);
      });
      console.log('');
      console.log(`💪 Estimated effort: ${this.estimateEffort(bestCandidate)} minutes`);
    } else {
      console.log('🎉 No obvious graduation candidates found!');
      console.log('   Consider working on any non-compliant modules or adding to whitelist.');
    }
  }

  private estimateEffort(candidate: GraduationCandidate): string {
    const blockerCount = candidate.blockers.length;
    if (blockerCount <= 1) return '5-15';
    if (blockerCount <= 2) return '15-30';
    if (blockerCount <= 3) return '30-60';
    return '60+';
  }
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  const tracker = new ModuleGraduationTracker();

  if (args.includes('--suggest')) {
    tracker.suggestNextTarget().catch(error => {
      console.error('❌ Target suggestion failed:', error);
      process.exit(1);
    });
  } else {
    tracker.checkGraduationCandidates().catch(error => {
      console.error('❌ Graduation check failed:', error);
      process.exit(1);
    });
  }
}

export { ModuleGraduationTracker };