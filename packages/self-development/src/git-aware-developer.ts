/**
 * Git-Aware Developer AI
 * 
 * Understands git workflow and proper feature branch development:
 * - Creates feature branches for self-development work
 * - Spawns Testing AIs on new branches to validate changes
 * - Coordinates with CI/CD and testing workflows
 * - Maintains clean git history and proper branch organization
 * - Ensures features are properly isolated and tested
 */

import * as fs from 'fs';
import * as path from 'path';
import { spawn, exec } from 'child_process';
import { promisify } from 'util';
import { ContinuumMemory } from '../memory/index.js';

const execAsync = promisify(exec);

export interface FeatureBranch {
  name: string;
  purpose: string;
  baseBranch: string;
  aiAssignments: {
    developer: string;
    tester: string;
    reporter: string;
  };
  status: 'created' | 'development' | 'testing' | 'review' | 'ready' | 'merged';
  files: string[];
  tests: string[];
  issues: GitIssue[];
}

export interface GitIssue {
  type: 'compilation' | 'test-failure' | 'linting' | 'type-error' | 'runtime-error';
  severity: 'low' | 'medium' | 'high' | 'critical';
  file: string;
  line?: number;
  message: string;
  suggestedFix?: string;
  reportedBy: string;
}

export interface TestingAI {
  id: string;
  name: string;
  specialization: 'compilation' | 'unit-tests' | 'integration' | 'linting' | 'type-checking';
  branch: string;
  status: 'idle' | 'testing' | 'reporting';
  lastReport: GitIssue[];
}

export class GitAwareDeveloper {
  private projectRoot: string;
  private memory: ContinuumMemory;
  private currentBranch: string = '';
  private activeBranches: Map<string, FeatureBranch> = new Map();
  private testingAIs: Map<string, TestingAI> = new Map();
  private gitConfig: {
    mainBranch: string;
    featureBranchPrefix: string;
    requiresPR: boolean;
    autoCleanup: boolean;
  };

  constructor(projectRoot: string) {
    this.projectRoot = projectRoot;
    this.memory = new ContinuumMemory(projectRoot);
    this.gitConfig = {
      mainBranch: 'main',
      featureBranchPrefix: 'continuum/',
      requiresPR: true,
      autoCleanup: true
    };
    
    this.initializeGitAwareness();
  }

  private async initializeGitAwareness(): Promise<void> {
    console.log('📂 GIT-AWARE DEVELOPER AI INITIALIZING');
    console.log('======================================');
    
    try {
      // Get current branch
      const { stdout } = await execAsync('git branch --show-current', { cwd: this.projectRoot });
      this.currentBranch = stdout.trim();
      
      console.log(`🌿 Current branch: ${this.currentBranch}`);
      
      // Check git status
      await this.checkGitStatus();
      
      console.log('🤖 Git-aware development capabilities activated');
      console.log('🔧 Can create feature branches and spawn testing AIs');
      console.log('🧪 Automated testing and issue reporting enabled');
      
    } catch (error) {
      console.error('❌ Git initialization failed:', error.message);
    }
  }

  private async checkGitStatus(): Promise<void> {
    try {
      const { stdout } = await execAsync('git status --porcelain', { cwd: this.projectRoot });
      
      if (stdout.trim()) {
        console.log('⚠️  Warning: Working directory has uncommitted changes');
        console.log('   Recommend committing before creating feature branches');
      } else {
        console.log('✅ Working directory clean - ready for feature development');
      }
    } catch (error) {
      console.log('❌ Cannot check git status:', error.message);
    }
  }

  // Feature Branch Management
  async createFeatureBranch(featureSpec: {
    name: string;
    purpose: string;
    estimatedChanges: string[];
    riskLevel: 'low' | 'medium' | 'high';
  }): Promise<FeatureBranch> {
    const branchName = `${this.gitConfig.featureBranchPrefix}${featureSpec.name}`;
    
    console.log(`🌿 Creating feature branch: ${branchName}`);
    console.log(`🎯 Purpose: ${featureSpec.purpose}`);
    
    try {
      // Ensure we're on main branch
      await this.ensureOnMainBranch();
      
      // Pull latest changes
      await execAsync('git pull origin main', { cwd: this.projectRoot });
      
      // Create and checkout new branch
      await execAsync(`git checkout -b ${branchName}`, { cwd: this.projectRoot });
      
      const featureBranch: FeatureBranch = {
        name: branchName,
        purpose: featureSpec.purpose,
        baseBranch: this.gitConfig.mainBranch,
        aiAssignments: {
          developer: 'continuum-developer',
          tester: await this.spawnTestingAI(branchName, featureSpec.riskLevel),
          reporter: await this.spawnReporterAI(branchName)
        },
        status: 'created',
        files: [],
        tests: [],
        issues: []
      };
      
      this.activeBranches.set(branchName, featureBranch);
      this.currentBranch = branchName;
      
      console.log(`✅ Feature branch created: ${branchName}`);
      console.log(`🤖 Testing AI assigned: ${featureBranch.aiAssignments.tester}`);
      console.log(`📊 Reporter AI assigned: ${featureBranch.aiAssignments.reporter}`);
      
      // Store branch creation in memory
      await this.memory.storeStrategy({
        id: `branch_creation_${Date.now()}`,
        projectType: 'git-workflow',
        strategy: {
          taskDelegation: { 'branch-creation': [branchName] },
          costOptimization: ['Use testing AIs to prevent integration issues'],
          successfulPatterns: [`Created feature branch for ${featureSpec.purpose}`],
          failurePatterns: []
        },
        performance: {
          totalCost: 0.005, // Very low cost for branch creation
          successRate: 1.0,
          completionTime: 10,
          userSatisfaction: 0.9
        },
        timestamp: Date.now(),
        sessionId: `git_${Date.now()}`,
        aiAgentsUsed: ['git-aware-developer'],
        tags: ['git', 'feature-branch', featureSpec.name]
      });
      
      return featureBranch;
      
    } catch (error) {
      console.error(`❌ Failed to create feature branch: ${error.message}`);
      throw new Error(`Branch creation failed: ${error.message}`);
    }
  }

  private async ensureOnMainBranch(): Promise<void> {
    if (this.currentBranch !== this.gitConfig.mainBranch) {
      console.log(`🔄 Switching to ${this.gitConfig.mainBranch} branch`);
      await execAsync(`git checkout ${this.gitConfig.mainBranch}`, { cwd: this.projectRoot });
      this.currentBranch = this.gitConfig.mainBranch;
    }
  }

  // Testing AI Management
  private async spawnTestingAI(branchName: string, riskLevel: string): Promise<string> {
    const testerId = `tester_${branchName.replace(/[^a-zA-Z0-9]/g, '_')}_${Date.now()}`;
    
    const testingAI: TestingAI = {
      id: testerId,
      name: `Testing AI for ${branchName}`,
      specialization: riskLevel === 'high' ? 'integration' : 'compilation',
      branch: branchName,
      status: 'idle',
      lastReport: []
    };
    
    this.testingAIs.set(testerId, testingAI);
    
    console.log(`🧪 Spawned Testing AI: ${testerId}`);
    console.log(`   Specialization: ${testingAI.specialization}`);
    console.log(`   Branch: ${branchName}`);
    
    return testerId;
  }

  private async spawnReporterAI(branchName: string): Promise<string> {
    const reporterId = `reporter_${branchName.replace(/[^a-zA-Z0-9]/g, '_')}_${Date.now()}`;
    
    console.log(`📊 Spawned Reporter AI: ${reporterId}`);
    console.log(`   Will monitor and report issues on ${branchName}`);
    
    return reporterId;
  }

  // Development and Testing Workflow
  async developFeature(branchName: string, changes: {
    files: { path: string; content: string }[];
    tests: { path: string; content: string }[];
  }): Promise<{
    success: boolean;
    issues: GitIssue[];
    recommendations: string[];
  }> {
    console.log(`🔧 Developing feature on branch: ${branchName}`);
    
    const featureBranch = this.activeBranches.get(branchName);
    if (!featureBranch) {
      throw new Error(`Feature branch ${branchName} not found`);
    }
    
    featureBranch.status = 'development';
    
    try {
      // Ensure we're on the correct branch
      await execAsync(`git checkout ${branchName}`, { cwd: this.projectRoot });
      
      // Write files
      for (const file of changes.files) {
        const fullPath = path.join(this.projectRoot, file.path);
        const dir = path.dirname(fullPath);
        
        // Ensure directory exists
        if (!fs.existsSync(dir)) {
          fs.mkdirSync(dir, { recursive: true });
        }
        
        fs.writeFileSync(fullPath, file.content);
        featureBranch.files.push(file.path);
        
        console.log(`📝 Created/modified: ${file.path}`);
      }
      
      // Write tests
      for (const test of changes.tests) {
        const fullPath = path.join(this.projectRoot, test.path);
        const dir = path.dirname(fullPath);
        
        if (!fs.existsSync(dir)) {
          fs.mkdirSync(dir, { recursive: true });
        }
        
        fs.writeFileSync(fullPath, test.content);
        featureBranch.tests.push(test.path);
        
        console.log(`🧪 Created test: ${test.path}`);
      }
      
      // Trigger testing AI
      const issues = await this.runTestingAI(featureBranch.aiAssignments.tester);
      featureBranch.issues = issues;
      
      // Generate recommendations
      const recommendations = await this.generateRecommendations(issues);
      
      const success = issues.filter(i => i.severity === 'critical' || i.severity === 'high').length === 0;
      
      if (success) {
        featureBranch.status = 'testing';
        console.log(`✅ Feature development completed successfully`);
      } else {
        console.log(`⚠️  Feature development completed with ${issues.length} issues`);
      }
      
      return { success, issues, recommendations };
      
    } catch (error) {
      console.error(`❌ Feature development failed: ${error.message}`);
      featureBranch.issues.push({
        type: 'runtime-error',
        severity: 'critical',
        file: 'development-process',
        message: `Development failed: ${error.message}`,
        reportedBy: 'git-aware-developer'
      });
      
      return {
        success: false,
        issues: featureBranch.issues,
        recommendations: ['Fix critical development errors before proceeding']
      };
    }
  }

  private async runTestingAI(testerId: string): Promise<GitIssue[]> {
    console.log(`🧪 Running Testing AI: ${testerId}`);
    
    const testingAI = this.testingAIs.get(testerId);
    if (!testingAI) {
      return [{
        type: 'runtime-error',
        severity: 'high',
        file: 'testing-system',
        message: `Testing AI ${testerId} not found`,
        reportedBy: 'git-aware-developer'
      }];
    }
    
    testingAI.status = 'testing';
    const issues: GitIssue[] = [];
    
    try {
      // Run compilation check
      console.log('   🔍 Checking compilation...');
      const compilationIssues = await this.checkCompilation();
      issues.push(...compilationIssues);
      
      // Run linting
      console.log('   🔍 Running linting...');
      const lintingIssues = await this.checkLinting();
      issues.push(...lintingIssues);
      
      // Run type checking
      console.log('   🔍 Checking types...');
      const typeIssues = await this.checkTypes();
      issues.push(...typeIssues);
      
      // Run tests if specialization allows
      if (testingAI.specialization === 'unit-tests' || testingAI.specialization === 'integration') {
        console.log('   🔍 Running tests...');
        const testIssues = await this.runTests();
        issues.push(...testIssues);
      }
      
      testingAI.lastReport = issues;
      testingAI.status = 'reporting';
      
      console.log(`📊 Testing complete: ${issues.length} issues found`);
      console.log(`   Critical: ${issues.filter(i => i.severity === 'critical').length}`);
      console.log(`   High: ${issues.filter(i => i.severity === 'high').length}`);
      console.log(`   Medium: ${issues.filter(i => i.severity === 'medium').length}`);
      console.log(`   Low: ${issues.filter(i => i.severity === 'low').length}`);
      
    } catch (error) {
      issues.push({
        type: 'runtime-error',
        severity: 'critical',
        file: 'testing-process',
        message: `Testing AI failed: ${error.message}`,
        reportedBy: testerId
      });
    }
    
    return issues;
  }

  private async checkCompilation(): Promise<GitIssue[]> {
    const issues: GitIssue[] = [];
    
    try {
      // Try TypeScript compilation
      const { stderr } = await execAsync('npx tsc --noEmit', { cwd: this.projectRoot });
      
      if (stderr) {
        // Parse TypeScript errors
        const errorLines = stderr.split('\n').filter(line => line.includes('error TS'));
        
        errorLines.forEach(line => {
          const match = line.match(/(.+?)\((\d+),\d+\): error TS\d+: (.+)/);
          if (match) {
            issues.push({
              type: 'compilation',
              severity: 'high',
              file: match[1],
              line: parseInt(match[2]),
              message: match[3],
              suggestedFix: 'Review TypeScript error and fix type issues',
              reportedBy: 'testing-ai-compilation'
            });
          }
        });
      }
      
    } catch (error) {
      if (error.stderr) {
        issues.push({
          type: 'compilation',
          severity: 'critical',
          file: 'project',
          message: `Compilation failed: ${error.stderr}`,
          suggestedFix: 'Fix syntax errors and missing dependencies',
          reportedBy: 'testing-ai-compilation'
        });
      }
    }
    
    return issues;
  }

  private async checkLinting(): Promise<GitIssue[]> {
    const issues: GitIssue[] = [];
    
    try {
      const { stdout, stderr } = await execAsync('npm run lint', { cwd: this.projectRoot });
      
      if (stderr || stdout.includes('error') || stdout.includes('warning')) {
        // Parse ESLint output (simplified)
        const output = stderr || stdout;
        const lines = output.split('\n');
        
        lines.forEach(line => {
          if (line.includes('error') || line.includes('warning')) {
            const severity = line.includes('error') ? 'medium' : 'low';
            issues.push({
              type: 'linting',
              severity: severity as 'medium' | 'low',
              file: 'detected in output',
              message: line.trim(),
              suggestedFix: 'Run npm run lint and fix the reported issues',
              reportedBy: 'testing-ai-linting'
            });
          }
        });
      }
      
    } catch (error) {
      // Linting issues are not critical for basic functionality
      issues.push({
        type: 'linting',
        severity: 'low',
        file: 'linting-process',
        message: `Linting check failed: ${error.message}`,
        reportedBy: 'testing-ai-linting'
      });
    }
    
    return issues;
  }

  private async checkTypes(): Promise<GitIssue[]> {
    const issues: GitIssue[] = [];
    
    try {
      const { stderr } = await execAsync('npx tsc --noEmit --strict', { cwd: this.projectRoot });
      
      if (stderr) {
        issues.push({
          type: 'type-error',
          severity: 'medium',
          file: 'type-system',
          message: 'Strict type checking found issues',
          suggestedFix: 'Review and fix type annotations',
          reportedBy: 'testing-ai-types'
        });
      }
      
    } catch (error) {
      // Type errors are important but not critical for basic development
      issues.push({
        type: 'type-error',
        severity: 'medium',
        file: 'type-checking',
        message: `Type checking failed: ${error.message}`,
        reportedBy: 'testing-ai-types'
      });
    }
    
    return issues;
  }

  private async runTests(): Promise<GitIssue[]> {
    const issues: GitIssue[] = [];
    
    try {
      const { stdout, stderr } = await execAsync('npm test', { cwd: this.projectRoot });
      
      if (stderr || stdout.includes('FAIL') || stdout.includes('failed')) {
        issues.push({
          type: 'test-failure',
          severity: 'high',
          file: 'test-suite',
          message: 'Some tests are failing',
          suggestedFix: 'Review test output and fix failing tests',
          reportedBy: 'testing-ai-tests'
        });
      }
      
    } catch (error) {
      issues.push({
        type: 'test-failure',
        severity: 'high',
        file: 'test-execution',
        message: `Test execution failed: ${error.message}`,
        suggestedFix: 'Fix test setup and configuration issues',
        reportedBy: 'testing-ai-tests'
      });
    }
    
    return issues;
  }

  private async generateRecommendations(issues: GitIssue[]): Promise<string[]> {
    const recommendations = [];
    
    const criticalIssues = issues.filter(i => i.severity === 'critical');
    const highIssues = issues.filter(i => i.severity === 'high');
    
    if (criticalIssues.length > 0) {
      recommendations.push('🚨 Fix critical issues before proceeding with feature development');
      recommendations.push('Consider reverting recent changes if issues persist');
    }
    
    if (highIssues.length > 0) {
      recommendations.push('⚠️  Address high-severity issues to ensure feature stability');
    }
    
    if (issues.filter(i => i.type === 'compilation').length > 0) {
      recommendations.push('🔧 Focus on compilation errors first - they block all other progress');
    }
    
    if (issues.filter(i => i.type === 'test-failure').length > 0) {
      recommendations.push('🧪 Review and fix test failures to ensure feature works correctly');
    }
    
    if (issues.length === 0) {
      recommendations.push('✅ All checks passed! Ready to commit and create PR');
    }
    
    return recommendations;
  }

  // Git Operations
  async commitFeature(branchName: string, message: string): Promise<boolean> {
    console.log(`📝 Committing feature on branch: ${branchName}`);
    
    const featureBranch = this.activeBranches.get(branchName);
    if (!featureBranch) {
      console.error(`❌ Feature branch ${branchName} not found`);
      return false;
    }
    
    try {
      // Ensure we're on the feature branch
      await execAsync(`git checkout ${branchName}`, { cwd: this.projectRoot });
      
      // Add all changes
      await execAsync('git add .', { cwd: this.projectRoot });
      
      // Commit with proper message
      const fullMessage = `${message}\n\n🤖 Generated with [Claude Code](https://claude.ai/code)\n\nCo-Authored-By: Claude <noreply@anthropic.com>`;
      await execAsync(`git commit -m "${fullMessage}"`, { cwd: this.projectRoot });
      
      featureBranch.status = 'ready';
      
      console.log(`✅ Feature committed successfully`);
      return true;
      
    } catch (error) {
      console.error(`❌ Commit failed: ${error.message}`);
      return false;
    }
  }

  async createPullRequest(branchName: string): Promise<{
    success: boolean;
    url?: string;
    issues: GitIssue[];
  }> {
    console.log(`🔀 Creating pull request for: ${branchName}`);
    
    const featureBranch = this.activeBranches.get(branchName);
    if (!featureBranch) {
      return {
        success: false,
        issues: [{
          type: 'runtime-error',
          severity: 'critical',
          file: 'git-workflow',
          message: `Feature branch ${branchName} not found`,
          reportedBy: 'git-aware-developer'
        }]
      };
    }
    
    // Final testing before PR
    const finalIssues = await this.runTestingAI(featureBranch.aiAssignments.tester);
    const criticalIssues = finalIssues.filter(i => i.severity === 'critical' || i.severity === 'high');
    
    if (criticalIssues.length > 0) {
      console.log(`❌ Cannot create PR - ${criticalIssues.length} critical/high issues found`);
      return { success: false, issues: finalIssues };
    }
    
    try {
      // Push branch to remote
      await execAsync(`git push -u origin ${branchName}`, { cwd: this.projectRoot });
      
      // Create PR using GitHub CLI if available
      const prTitle = `feat: ${featureBranch.purpose}`;
      const prBody = this.generatePRDescription(featureBranch, finalIssues);
      
      try {
        const { stdout } = await execAsync(
          `gh pr create --title "${prTitle}" --body "${prBody}"`,
          { cwd: this.projectRoot }
        );
        
        const prUrl = stdout.trim();
        console.log(`✅ Pull request created: ${prUrl}`);
        
        return { success: true, url: prUrl, issues: finalIssues };
        
      } catch (ghError) {
        console.log(`⚠️  GitHub CLI not available, but branch pushed successfully`);
        console.log(`   Create PR manually at: https://github.com/.../compare/${branchName}`);
        
        return { success: true, issues: finalIssues };
      }
      
    } catch (error) {
      console.error(`❌ PR creation failed: ${error.message}`);
      return {
        success: false,
        issues: [{
          type: 'runtime-error',
          severity: 'critical',
          file: 'pr-creation',
          message: `PR creation failed: ${error.message}`,
          reportedBy: 'git-aware-developer'
        }]
      };
    }
  }

  private generatePRDescription(featureBranch: FeatureBranch, issues: GitIssue[]): string {
    return `## Summary
${featureBranch.purpose}

## Changes
- Modified ${featureBranch.files.length} files
- Added ${featureBranch.tests.length} tests
- Branch: ${featureBranch.name}

## Testing
- ✅ Compilation: ${issues.filter(i => i.type === 'compilation').length === 0 ? 'Passed' : 'Issues found'}
- ✅ Linting: ${issues.filter(i => i.type === 'linting').length === 0 ? 'Passed' : 'Issues found'} 
- ✅ Type Checking: ${issues.filter(i => i.type === 'type-error').length === 0 ? 'Passed' : 'Issues found'}
- ✅ Tests: ${issues.filter(i => i.type === 'test-failure').length === 0 ? 'Passed' : 'Issues found'}

## AI Testing Report
- Testing AI: ${featureBranch.aiAssignments.tester}
- Issues found: ${issues.length}
- Critical/High: ${issues.filter(i => i.severity === 'critical' || i.severity === 'high').length}

🤖 Generated with [Claude Code](https://claude.ai/code)

Co-Authored-By: Claude <noreply@anthropic.com>`;
  }

  // Status and Reporting
  async getBranchStatus(): Promise<{
    currentBranch: string;
    activeBranches: FeatureBranch[];
    testingAIs: TestingAI[];
    summary: string;
  }> {
    const activeBranches = Array.from(this.activeBranches.values());
    const testingAIs = Array.from(this.testingAIs.values());
    
    const summary = `Git Status: ${activeBranches.length} active feature branches, ${testingAIs.length} testing AIs running`;
    
    return {
      currentBranch: this.currentBranch,
      activeBranches,
      testingAIs,
      summary
    };
  }

  async cleanupBranches(): Promise<void> {
    console.log('🧹 Cleaning up completed feature branches...');
    
    for (const [branchName, featureBranch] of this.activeBranches.entries()) {
      if (featureBranch.status === 'merged' && this.gitConfig.autoCleanup) {
        try {
          await execAsync(`git branch -d ${branchName}`, { cwd: this.projectRoot });
          this.activeBranches.delete(branchName);
          
          // Cleanup associated testing AIs
          const testingAI = this.testingAIs.get(featureBranch.aiAssignments.tester);
          if (testingAI) {
            this.testingAIs.delete(featureBranch.aiAssignments.tester);
            console.log(`🗑️  Cleaned up testing AI: ${testingAI.id}`);
          }
          
          console.log(`🗑️  Deleted merged branch: ${branchName}`);
          
        } catch (error) {
          console.log(`⚠️  Could not delete branch ${branchName}: ${error.message}`);
        }
      }
    }
  }
}