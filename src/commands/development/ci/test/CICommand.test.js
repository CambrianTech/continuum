/**
 * CICommand Tests - GitHub Actions integration testing
 * Validates modular CI workflows and issue automation
 */

import test from 'node:test';
import assert from 'node:assert';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

// ES module path handling
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Import the command under test (CommonJS module)
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const CICommand = require('../CICommand.cjs');

test.describe('CICommand Module Tests', () => {
  
  test('should have correct command definition', () => {
    const definition = CICommand.getDefinition();
    
    assert.strictEqual(definition.name, 'ci');
    assert.strictEqual(definition.icon, '🚀');
    assert.strictEqual(definition.category, 'development');
    assert.ok(definition.parameters.action);
    assert.ok(Array.isArray(definition.parameters.action.enum));
  });

  test('should load CI configuration files', () => {
    const configPath = path.join(__dirname, '..', 'config', 'ci_flows.json');
    assert.ok(fs.existsSync(configPath), 'CI flows configuration should exist');
    
    const config = CICommand.loadConfig('ci_flows.json');
    assert.ok(config.issue_transitions, 'Should have issue transition rules');
    assert.ok(config.automated_checks, 'Should have automated check definitions');
  });

  test('should extract issue references from commit messages', () => {
    const testCases = [
      {
        message: 'Fix bug in screenshot command fixes #123',
        expected: { closing: [123], updating: [] }
      },
      {
        message: 'Update docs ref #456 and closes #789',
        expected: { closing: [789], updating: [456] }
      },
      {
        message: 'Regular commit message with no references',
        expected: { closing: [], updating: [] }
      },
      {
        message: 'Resolves #100 and relates to #200',
        expected: { closing: [100], updating: [200] }
      }
    ];

    testCases.forEach(({ message, expected }) => {
      const result = CICommand.extractIssueReferences(message);
      assert.deepStrictEqual(result, expected, `Failed for message: "${message}"`);
    });
  });

  test('should parse test output correctly', () => {
    const testOutput = `
> continuum@1.0.0 test
> node --test __tests__/**/*.test.js

✓ Screenshot command tests (1ms)
✓ Issues command tests (2ms)
✗ CI command tests - failing test (5ms)
  AssertionError: Expected true but got false
✓ Another passing test (1ms)

Tests: 3 passed, 1 failed, 4 total
    `;

    const result = CICommand.parseTestOutput(testOutput);
    
    assert.strictEqual(result.passed, 3);
    assert.strictEqual(result.failed, 1);
    assert.strictEqual(result.total, 4);
    assert.strictEqual(result.success, false);
  });

  test('should handle analyze_commit action', async () => {
    const params = {
      action: 'analyze_commit',
      commit_sha: 'test-sha-123'
    };

    // Mock the getCommitInfo method directly instead of execSync
    const originalGetCommitInfo = CICommand.getCommitInfo;
    CICommand.getCommitInfo = (sha) => {
      return {
        sha: 'test-sha-123',
        message: 'Fix screenshot bug fixes #123',
        author: 'TestUser',
        date: '2025-06-20 14:00:00 +0000'
      };
    };

    try {
      const result = await CICommand.execute(JSON.stringify(params), null);
      
      assert.ok(result.success);
      assert.strictEqual(result.data.commit.sha, 'test-sha-123');
      assert.ok(result.data.issue_references.closing.includes(123));
    } finally {
      // Restore original method
      CICommand.getCommitInfo = originalGetCommitInfo;
    }
  });

  test('should validate required configuration files exist', () => {
    const configDir = path.join(__dirname, '..', 'config');
    const workflowsDir = path.join(__dirname, '..', 'workflows');
    
    // Check config files
    assert.ok(fs.existsSync(path.join(configDir, 'ci_flows.json')));
    
    // Check workflow templates
    assert.ok(fs.existsSync(path.join(workflowsDir, 'test-and-issues.yml')));
  });

  test('should follow modular design principles', () => {
    const commandFile = path.join(__dirname, '..', 'CICommand.cjs');
    const commandContent = fs.readFileSync(commandFile, 'utf8');
    
    // Should delegate to modular commands, not duplicate logic
    assert.ok(commandContent.includes('runModularCommand'), 'Should delegate to other commands');
    
    // Should not contain hardcoded GitHub API calls
    assert.ok(!commandContent.includes('github.com/api'), 'Should use modular GitHub integration');
    
    // Should load external configuration
    assert.ok(commandContent.includes('loadConfig'), 'Should use external configuration');
  });

  test('should handle error cases gracefully', async () => {
    const params = { action: 'invalid_action' };
    
    const result = await CICommand.execute(JSON.stringify(params), null);
    
    assert.ok(!result.success);
    assert.ok(result.error.includes('Unknown action'));
  });

  test('should create proper CI results structure', async () => {
    const params = {
      action: 'full_check',
      commit_sha: 'test-123',
      pr_number: '456'
    };

    // Mock the modular command calls
    const originalRunModularCommand = CICommand.runModularCommand;
    const originalGetCommitInfo = CICommand.getCommitInfo;
    
    CICommand.runModularCommand = async (command, params) => {
      return { success: true, command, params };
    };
    
    CICommand.getCommitInfo = (sha) => {
      return {
        sha: 'test-123',
        message: 'Test commit message',
        author: 'TestUser',
        date: '2025-06-20 14:00:00 +0000'
      };
    };

    try {
      const result = await CICommand.execute(JSON.stringify(params), null);
      
      assert.ok(result.success);
      assert.strictEqual(result.data.commit_sha, 'test-123');
      assert.ok(result.data.tests);
      assert.ok(result.data.issues);
    } finally {
      // Restore original methods
      CICommand.runModularCommand = originalRunModularCommand;
      CICommand.getCommitInfo = originalGetCommitInfo;
    }
  });

});