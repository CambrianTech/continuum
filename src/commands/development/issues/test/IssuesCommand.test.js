/**
 * IssuesCommand Tests - Self-contained module testing
 * Validates AI-driven issue management functionality
 */

const test = require('node:test');
const assert = require('node:assert');
const path = require('path');
const fs = require('fs');

// Import the command under test
const IssuesCommand = require('../IssuesCommand.cjs');

test.describe('IssuesCommand Module Tests', () => {
  
  test('should have correct command definition', () => {
    const definition = IssuesCommand.getDefinition();
    
    assert.strictEqual(definition.name, 'issues');
    assert.strictEqual(definition.icon, '🎯');
    assert.strictEqual(definition.category, 'development');
    assert.ok(definition.parameters.action);
    assert.ok(Array.isArray(definition.parameters.action.enum));
  });

  test('should load external configuration files', () => {
    const messages = IssuesCommand.loadConfig('messages.json');
    const githubConfig = IssuesCommand.loadConfig('github_api.json');
    
    assert.ok(messages.dashboard);
    assert.ok(messages.categories);
    assert.ok(githubConfig.endpoints);
    assert.ok(githubConfig.headers);
  });

  test('should load external template files', () => {
    const dashboardTemplate = IssuesCommand.loadTemplate('dashboard.html');
    const issueTemplate = IssuesCommand.loadTemplate('issue_body.md');
    
    assert.ok(dashboardTemplate.includes('<html>'));
    assert.ok(issueTemplate.includes('## Issue Description'));
  });

  test('should extract agent from connection', () => {
    const mockContinuum = {
      connection: { agent: 'TestAgent' },
      clients: [{ agent: 'FallbackAgent' }]
    };
    
    const agent = IssuesCommand.getAgentFromConnection(mockContinuum);
    assert.strictEqual(agent, 'TestAgent');
  });

  test('should fallback to client info for agent', () => {
    const mockContinuum = {
      clients: [{ agent: 'ClientAgent', name: 'TestClient' }]
    };
    
    const agent = IssuesCommand.getAgentFromConnection(mockContinuum);
    assert.strictEqual(agent, 'ClientAgent');
  });

  test('should categorize issue markers correctly', () => {
    assert.strictEqual(IssuesCommand.categorizeLine('🧹 cleanup needed'), 'cleanup');
    assert.strictEqual(IssuesCommand.categorizeLine('🌀 investigate this'), 'investigation');
    assert.strictEqual(IssuesCommand.categorizeLine('🔥 test failing'), 'test-failure');
    assert.strictEqual(IssuesCommand.categorizeLine('📦 refactor needed'), 'architecture');
    assert.strictEqual(IssuesCommand.categorizeLine('🎯 enhancement idea'), 'enhancement');
  });

  test('should extract issue markers from content', () => {
    const content = `
# Test File
Some regular content
🧹 This needs cleanup
More content
🔥 This test is broken
🎯 This could be enhanced
`;
    
    const markers = IssuesCommand.extractIssueMarkers(content);
    assert.strictEqual(markers.length, 3);
    assert.strictEqual(markers[0].category, 'cleanup');
    assert.strictEqual(markers[1].category, 'test-failure');
    assert.strictEqual(markers[2].category, 'enhancement');
  });

  test('should execute dashboard action', async () => {
    const params = { action: 'dashboard' };
    const mockContinuum = { connection: { agent: 'TestAgent' } };
    
    const result = await IssuesCommand.execute(JSON.stringify(params), mockContinuum);
    
    assert.ok(result.success);
    assert.strictEqual(result.data.action, 'dashboard');
    assert.strictEqual(result.data.agent, 'TestAgent');
  });

  test('should execute sync action', async () => {
    const params = { action: 'sync' };
    const mockContinuum = { connection: { agent: 'TestAgent' } };
    
    // Mock FILES.md existence check
    const originalExistsSync = fs.existsSync;
    const originalReadFileSync = fs.readFileSync;
    
    fs.existsSync = () => true;
    fs.readFileSync = () => '🧹 test content\n🔥 another issue';
    
    try {
      const result = await IssuesCommand.execute(JSON.stringify(params), mockContinuum);
      
      assert.ok(result.success);
      assert.strictEqual(result.data.action, 'sync');
      assert.strictEqual(result.data.issues_found, 2);
    } finally {
      // Restore original functions
      fs.existsSync = originalExistsSync;
      fs.readFileSync = originalReadFileSync;
    }
  });

  test('should validate required template files exist', () => {
    const templatesDir = path.join(__dirname, '..', 'templates');
    const configDir = path.join(__dirname, '..', 'config');
    
    // Check template files
    assert.ok(fs.existsSync(path.join(templatesDir, 'dashboard.html')));
    assert.ok(fs.existsSync(path.join(templatesDir, 'issue_body.md')));
    
    // Check config files
    assert.ok(fs.existsSync(path.join(configDir, 'messages.json')));
    assert.ok(fs.existsSync(path.join(configDir, 'github_api.json')));
  });

  test('should handle invalid action gracefully', async () => {
    const params = { action: 'invalid_action' };
    const mockContinuum = { connection: { agent: 'TestAgent' } };
    
    const result = await IssuesCommand.execute(JSON.stringify(params), mockContinuum);
    
    assert.ok(!result.success);
    assert.ok(result.error.includes('Unknown action'));
  });

  test('should follow MIME type separation', () => {
    // Verify no language mixing in main command file
    const commandFile = path.join(__dirname, '..', 'IssuesCommand.cjs');
    const commandContent = fs.readFileSync(commandFile, 'utf8');
    
    // Should not contain hardcoded HTML
    assert.ok(!commandContent.includes('<html>'));
    assert.ok(!commandContent.includes('<div>'));
    
    // Should not contain hardcoded markdown
    assert.ok(!commandContent.includes('## Issue Description'));
    
    // Should not contain hardcoded user messages
    assert.ok(!commandContent.includes('Issues Dashboard'));
    assert.ok(commandContent.includes('loadTemplate'));
    assert.ok(commandContent.includes('loadConfig'));
  });

});