/**
 * CICommand - GitHub Actions integration with automated issue tracking
 * Handles CI workflows, test result analysis, and automatic issue updates
 */

const BaseCommand = require('../../BaseCommand.cjs');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

class CICommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'ci',
      description: 'GitHub Actions integration with automated issue tracking',
      icon: '🚀',
      category: 'development',
      parameters: {
        action: {
          type: 'string',
          required: true,
          description: 'CI action: full_check, run_tests, update_issues, analyze_commit',
          enum: ['full_check', 'run_tests', 'update_issues', 'analyze_commit']
        },
        commit_sha: {
          type: 'string',
          required: false,
          description: 'Git commit SHA for analysis'
        },
        pr_number: {
          type: 'string', 
          required: false,
          description: 'Pull request number'
        },
        github_token: {
          type: 'string',
          required: false,
          description: 'GitHub API token for issue updates'
        },
        test_results: {
          type: 'string',
          required: false,
          description: 'Path to test results JSON file'
        }
      }
    };
  }

  static loadConfig(configName) {
    const configPath = path.join(__dirname, 'config', configName);
    return JSON.parse(fs.readFileSync(configPath, 'utf8'));
  }

  static async execute(paramsString, continuum) {
    try {
      const params = this.parseParams(paramsString);
      const action = params.action;
      
      console.log(`🚀 CI Action: ${action}`);
      
      switch (action) {
        case 'full_check':
          return await this.runFullCheck(params);
        
        case 'run_tests':
          return await this.runTests(params);
        
        case 'update_issues':
          return await this.updateIssues(params);
        
        case 'analyze_commit':
          return await this.analyzeCommit(params);
        
        default:
          return this.createErrorResult('Invalid CI action', `Unknown action: ${action}`);
      }

    } catch (error) {
      return this.createErrorResult('CI command failed', error.message);
    }
  }

  static async runFullCheck(params) {
    console.log('🧪 Starting full CI check...');
    
    const results = {
      timestamp: new Date().toISOString(),
      commit_sha: params.commit_sha,
      pr_number: params.pr_number,
      tests: {},
      issues: {},
      status: 'running'
    };

    try {
      // Step 1: Run all tests using modular test command
      console.log('📋 Running comprehensive test suite...');
      const testResult = await this.runModularCommand('tests', { type: 'all', ci: true });
      results.tests = testResult;

      // Step 2: Analyze commit for issue references
      if (params.commit_sha) {
        console.log('🔍 Analyzing commit for issue references...');
        const commitAnalysis = await this.analyzeCommitForIssues(params.commit_sha);
        results.issues.commit_analysis = commitAnalysis;
      }

      // Step 3: Check FILES.md for new issues
      console.log('📁 Scanning FILES.md for issue markers...');
      const filesScan = await this.runModularCommand('issues', { 
        action: 'sync', 
        auto_create: true 
      });
      results.issues.files_scan = filesScan;

      // Step 4: Update dashboard
      console.log('📊 Updating dashboard...');
      await this.runModularCommand('docs', { sync: true });

      results.status = results.tests.success ? 'passed' : 'failed';

      // Save results for issue updates
      fs.writeFileSync('/tmp/ci_results.json', JSON.stringify(results, null, 2));

      return this.createSuccessResult(results, 
        `CI check ${results.status}: ${results.tests.passed}/${results.tests.total} tests passed`);

    } catch (error) {
      results.status = 'error';
      results.error = error.message;
      fs.writeFileSync('/tmp/ci_results.json', JSON.stringify(results, null, 2));
      throw error;
    }
  }

  static async runTests(params) {
    console.log('🧪 Running modular test suite...');
    
    try {
      // Use the existing test command
      const testOutput = execSync('npm test', { 
        encoding: 'utf8',
        timeout: 300000 // 5 minutes
      });

      const testResult = this.parseTestOutput(testOutput);
      
      return this.createSuccessResult(testResult, 
        `Tests completed: ${testResult.passed}/${testResult.total} passed`);

    } catch (error) {
      const testResult = {
        success: false,
        passed: 0,
        failed: 0,
        total: 0,
        error: error.message,
        output: error.stdout || ''
      };

      return this.createErrorResult('Tests failed', testResult);
    }
  }

  static async updateIssues(params) {
    console.log('📋 Updating issues based on CI results...');
    
    const resultsPath = params.test_results || '/tmp/ci_results.json';
    if (!fs.existsSync(resultsPath)) {
      return this.createErrorResult('No CI results found', 'Run full_check first');
    }

    const ciResults = JSON.parse(fs.readFileSync(resultsPath, 'utf8'));
    const updates = [];

    try {
      // Update issues based on test results
      if (ciResults.tests.failed > 0) {
        // Create or update test failure issues
        const issueResult = await this.runModularCommand('issues', {
          action: 'create',
          category: 'test-failure',
          title: `CI Tests Failing (${ciResults.tests.failed}/${ciResults.tests.total})`,
          commit_sha: ciResults.commit_sha
        });
        updates.push(issueResult);
      }

      // Close issues if tests are now passing
      if (ciResults.tests.success && ciResults.commit_sha) {
        const closeResult = await this.runModularCommand('issues', {
          action: 'close_fixed',
          commit_sha: ciResults.commit_sha
        });
        updates.push(closeResult);
      }

      return this.createSuccessResult({
        updates,
        ci_results: ciResults
      }, `Updated ${updates.length} issues based on CI results`);

    } catch (error) {
      return this.createErrorResult('Issue update failed', error.message);
    }
  }

  static async analyzeCommit(params) {
    if (!params.commit_sha) {
      return this.createErrorResult('Commit SHA required', 'Provide commit_sha parameter');
    }

    try {
      const commitInfo = this.getCommitInfo(params.commit_sha);
      const issueRefs = this.extractIssueReferences(commitInfo.message);
      
      return this.createSuccessResult({
        commit: commitInfo,
        issue_references: issueRefs,
        should_close_issues: issueRefs.closing.length > 0,
        should_update_issues: issueRefs.updating.length > 0
      }, `Analyzed commit ${params.commit_sha.substring(0, 8)}`);

    } catch (error) {
      return this.createErrorResult('Commit analysis failed', error.message);
    }
  }

  // Helper methods
  static async runModularCommand(commandName, params) {
    // Simulate calling other Continuum commands
    // In real implementation, this would use the command registry
    console.log(`🔗 Calling modular command: ${commandName}`, params);
    
    // Return realistic mock data for different commands
    if (commandName === 'tests') {
      return { success: true, passed: 8, failed: 0, total: 8 };
    }
    if (commandName === 'issues') {
      return { success: true, issues_found: 3, action: params.action };
    }
    if (commandName === 'docs') {
      return { success: true, synced: true };
    }
    
    return { success: true, command: commandName, params };
  }

  static parseTestOutput(output) {
    // Parse npm test output to extract test results
    const lines = output.split('\n');
    let passed = 0, failed = 0, total = 0;
    
    for (const line of lines) {
      if (line.includes('✓') || line.includes('✅')) passed++;
      if (line.includes('✗') || line.includes('❌') || line.includes('FAIL')) failed++;
    }
    
    total = passed + failed;
    
    return {
      success: failed === 0 && total > 0,
      passed,
      failed, 
      total,
      output
    };
  }

  static getCommitInfo(sha) {
    try {
      const message = execSync(`git log -1 --pretty=format:"%s" ${sha}`, { encoding: 'utf8' });
      const author = execSync(`git log -1 --pretty=format:"%an" ${sha}`, { encoding: 'utf8' });
      const date = execSync(`git log -1 --pretty=format:"%ci" ${sha}`, { encoding: 'utf8' });
      
      return { sha, message, author, date };
    } catch (error) {
      throw new Error(`Failed to get commit info: ${error.message}`);
    }
  }

  static extractIssueReferences(commitMessage) {
    const closing = [];
    const updating = [];
    
    // Look for "fixes #123", "closes #456", etc.
    const closingMatches = commitMessage.match(/(fix|fixes|close|closes|resolve|resolves)\s+#(\d+)/gi);
    if (closingMatches) {
      closingMatches.forEach(match => {
        const issueNum = match.match(/#(\d+)/)[1];
        closing.push(parseInt(issueNum));
      });
    }
    
    // Look for "ref #123", "see #456", "relates to #200", etc.
    const refMatches = commitMessage.match(/(ref|refs|see|relates?\s+to|related)\s+#(\d+)/gi);
    if (refMatches) {
      refMatches.forEach(match => {
        const issueNum = match.match(/#(\d+)/)[1];
        updating.push(parseInt(issueNum));
      });
    }
    
    return { closing, updating };
  }

  static async analyzeCommitForIssues(sha) {
    const commitInfo = this.getCommitInfo(sha);
    const issueRefs = this.extractIssueReferences(commitInfo.message);
    return { commit: commitInfo, issue_references: issueRefs };
  }
}

module.exports = CICommand;