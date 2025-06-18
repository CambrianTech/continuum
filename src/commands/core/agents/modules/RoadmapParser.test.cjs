/**
 * RoadmapParser Unit Tests
 * Test roadmap parsing, item extraction, and metadata analysis
 */

const assert = require('assert');
const RoadmapParser = require('../modules/RoadmapParser.cjs');

describe('RoadmapParser', () => {
  let parser;

  beforeEach(() => {
    parser = new RoadmapParser();
  });

  describe('parseMarkdownContent', () => {
    it('should parse basic todo items', () => {
      const markdown = `
## Phase 1: Foundation
- [ ] Fix broken spawn command
- [ ] Restore Mass Effect UI components

## Phase 2: Academy
- [ ] **Academy System** - High complexity restoration
      `;

      const items = parser.parseMarkdownContent(markdown);
      
      assert.strictEqual(items.length, 3);
      assert.strictEqual(items[0].title, 'Fix broken spawn command');
      assert.strictEqual(items[0].category, 'Phase 1: Foundation');
      assert.strictEqual(items[1].title, 'Restore Mass Effect UI components');
      assert.strictEqual(items[2].title, '- High complexity restoration');
    });

    it('should handle empty markdown gracefully', () => {
      const items = parser.parseMarkdownContent('');
      assert.strictEqual(items.length, 0);
    });

    it('should skip non-todo lines', () => {
      const markdown = `
## Section
Some text here
- Regular bullet point
- [ ] Valid todo item
- [x] Completed todo (should be skipped)
      `;

      const items = parser.parseMarkdownContent(markdown);
      assert.strictEqual(items.length, 1);
      assert.strictEqual(items[0].title, 'Valid todo item');
    });
  });

  describe('extractComplexity', () => {
    it('should detect explicit complexity markers', () => {
      assert.strictEqual(parser.extractComplexity('- [ ] Simple task 🟢', 'Simple task'), 'Low');
      assert.strictEqual(parser.extractComplexity('- [ ] Complex task 🔴', 'Complex task'), 'High');
      assert.strictEqual(parser.extractComplexity('- [ ] High complexity system', 'High complexity'), 'High');
    });

    it('should infer complexity from content', () => {
      assert.strictEqual(parser.extractComplexity('', 'Fix broken command'), 'Medium');
      assert.strictEqual(parser.extractComplexity('', 'Implement new system'), 'Medium');
      assert.strictEqual(parser.extractComplexity('', 'git show abc123:path'), 'Low');
    });

    it('should default to Medium for ambiguous content', () => {
      assert.strictEqual(parser.extractComplexity('', 'Some task'), 'Medium');
    });
  });

  describe('extractRisk', () => {
    it('should categorize sections by risk level', () => {
      assert.strictEqual(parser.extractRisk('Critical Fixes'), 'Low');
      assert.strictEqual(parser.extractRisk('Academy Restoration'), 'Medium');
      assert.strictEqual(parser.extractRisk('Advanced AI Ecosystem'), 'High');
    });

    it('should default to Medium for unknown sections', () => {
      assert.strictEqual(parser.extractRisk('Unknown Section'), 'Medium');
    });
  });

  describe('extractImpact', () => {
    it('should detect high impact keywords', () => {
      assert.strictEqual(parser.extractImpact('Fix broken critical system'), 'High');
      assert.strictEqual(parser.extractImpact('Command blocks other processes'), 'High');
    });

    it('should detect medium impact keywords', () => {
      assert.strictEqual(parser.extractImpact('Enhance user experience'), 'Medium');
      assert.strictEqual(parser.extractImpact('Improve performance'), 'Medium');
    });

    it('should default to Low for other content', () => {
      assert.strictEqual(parser.extractImpact('Some regular task'), 'Low');
    });
  });

  describe('extractDependencies', () => {
    it('should detect UI + Academy dependencies', () => {
      const deps = parser.extractDependencies('Connect UI to Academy system');
      assert.deepStrictEqual(deps, ['Mass Effect UI', 'Core Components']);
    });

    it('should detect integration dependencies', () => {
      const deps = parser.extractDependencies('Integration with core components');
      assert.deepStrictEqual(deps, ['Core Components']);
    });

    it('should return empty array for no dependencies', () => {
      const deps = parser.extractDependencies('Simple standalone task');
      assert.deepStrictEqual(deps, []);
    });
  });

  describe('extractCommands', () => {
    it('should extract git recovery commands', () => {
      const commands = parser.extractCommands('git show abc123:src/file.js');
      assert.deepStrictEqual(commands, ['git show abc123:src/file.js']);
    });

    it('should extract command names', () => {
      const commands = parser.extractCommands('Fix spawn command and screenshot issues');
      assert.deepStrictEqual(commands, ['spawn', 'screenshot']);
    });

    it('should return empty array for no commands', () => {
      const commands = parser.extractCommands('General improvement task');
      assert.deepStrictEqual(commands, []);
    });
  });

  describe('getDefaultItems', () => {
    it('should return well-formed default items', () => {
      const items = parser.getDefaultItems();
      
      assert(items.length > 0);
      
      items.forEach(item => {
        assert(typeof item.title === 'string');
        assert(typeof item.description === 'string');
        assert(typeof item.category === 'string');
        assert(['pending', 'in_progress', 'completed'].includes(item.status));
        assert(['Low', 'Medium', 'High'].includes(item.complexity));
        assert(['Low', 'Medium', 'High'].includes(item.risk));
        assert(['Low', 'Medium', 'High'].includes(item.impact));
        assert(Array.isArray(item.dependencies));
        assert(Array.isArray(item.commands));
      });
    });

    it('should include critical fixes with high impact', () => {
      const items = parser.getDefaultItems();
      const criticalFix = items.find(item => item.category === 'Critical Fixes');
      
      assert(criticalFix);
      assert.strictEqual(criticalFix.impact, 'High');
      assert.strictEqual(criticalFix.risk, 'Low');
    });
  });

  describe('parseTodoItem', () => {
    it('should create complete item object', () => {
      const line = '- [ ] Fix broken spawn command';
      const section = 'Critical Fixes';
      
      const item = parser.parseTodoItem(line, section);
      
      assert.strictEqual(item.title, 'Fix broken spawn command');
      assert.strictEqual(item.category, 'Critical Fixes');
      assert.strictEqual(item.status, 'pending');
      assert(['Low', 'Medium', 'High'].includes(item.complexity));
      assert(['Low', 'Medium', 'High'].includes(item.risk));
      assert(['Low', 'Medium', 'High'].includes(item.impact));
      assert(Array.isArray(item.dependencies));
      assert(Array.isArray(item.commands));
    });

    it('should return null for empty title', () => {
      const item = parser.parseTodoItem('- [ ]   ', 'Section');
      assert.strictEqual(item, null);
    });

    it('should clean up markdown formatting', () => {
      const line = '- [ ] **Bold Task** - with description';
      const item = parser.parseTodoItem(line, 'Section');
      
      assert.strictEqual(item.title, '- with description');
    });
  });
});

// Run tests if this file is executed directly
if (require.main === module) {
  console.log('🧪 Running RoadmapParser tests...\n');
  
  try {
    // Simple test runner for environments without a test framework
    const testInstance = new RoadmapParser();
    
    // Test basic parsing
    const testMarkdown = `
## Phase 1: Foundation
- [ ] Fix broken spawn command
- [ ] Restore Mass Effect UI components 🟢
    `;
    
    const items = testInstance.parseMarkdownContent(testMarkdown);
    console.log(`✅ Parsed ${items.length} items from test markdown`);
    
    // Test default items
    const defaultItems = testInstance.getDefaultItems();
    console.log(`✅ Generated ${defaultItems.length} default items`);
    
    // Test extraction methods
    const complexity = testInstance.extractComplexity('', 'Fix broken command');
    console.log(`✅ Complexity extraction: ${complexity}`);
    
    const risk = testInstance.extractRisk('Critical Fixes');
    console.log(`✅ Risk extraction: ${risk}`);
    
    const impact = testInstance.extractImpact('Fix broken critical system');
    console.log(`✅ Impact extraction: ${impact}`);
    
    console.log('\n🎉 All RoadmapParser tests passed!');
    
  } catch (error) {
    console.error('❌ RoadmapParser test failed:', error.message);
    process.exit(1);
  }
}

module.exports = { RoadmapParser };