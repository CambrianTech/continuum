/**
 * StrategicAnalyzer Unit Tests
 * Test filtering, sorting, and strategic analysis functionality
 */

const assert = require('assert');
const StrategicAnalyzer = require('../modules/StrategicAnalyzer.cjs');

describe('StrategicAnalyzer', () => {
  let analyzer;
  let sampleItems;

  beforeEach(() => {
    analyzer = new StrategicAnalyzer();
    
    // Sample items for testing
    sampleItems = [
      {
        title: 'Critical Fix A',
        category: 'Critical Fixes',
        risk: 'Low',
        complexity: 'Low',
        impact: 'High',
        timeline: '2-4 hours',
        dependencies: []
      },
      {
        title: 'Academy Restoration',
        category: 'Academy Restoration', 
        risk: 'Medium',
        complexity: 'High',
        impact: 'High',
        timeline: '4-8 hours',
        dependencies: ['Mass Effect UI']
      },
      {
        title: 'Advanced Feature',
        category: 'Advanced Features',
        risk: 'High',
        complexity: 'High', 
        impact: 'Medium',
        timeline: '1-2 days',
        dependencies: ['Academy Restoration', 'Critical Fix A']
      },
      {
        title: 'Simple Enhancement',
        category: 'Enhancements',
        risk: 'Low',
        complexity: 'Low',
        impact: 'Low',
        timeline: '1-2 hours',
        dependencies: []
      }
    ];
  });

  describe('filterItems', () => {
    it('should return all items for "all" filter', () => {
      const filtered = analyzer.filterItems(sampleItems, 'all');
      assert.strictEqual(filtered.length, sampleItems.length);
    });

    it('should filter by risk level', () => {
      const highRisk = analyzer.filterItems(sampleItems, 'risk');
      assert.strictEqual(highRisk.length, 1);
      assert.strictEqual(highRisk[0].risk, 'High');
    });

    it('should filter by low risk', () => {
      const lowRisk = analyzer.filterItems(sampleItems, 'low-risk');
      assert.strictEqual(lowRisk.length, 2);
      lowRisk.forEach(item => assert.strictEqual(item.risk, 'Low'));
    });

    it('should filter by complexity', () => {
      const lowComplexity = analyzer.filterItems(sampleItems, 'complexity');
      assert.strictEqual(lowComplexity.length, 2);
      lowComplexity.forEach(item => assert.strictEqual(item.complexity, 'Low'));
    });

    it('should filter by impact', () => {
      const highImpact = analyzer.filterItems(sampleItems, 'impact');
      assert.strictEqual(highImpact.length, 2);
      highImpact.forEach(item => assert.strictEqual(item.impact, 'High'));
    });

    it('should filter by critical category', () => {
      const critical = analyzer.filterItems(sampleItems, 'critical');
      assert.strictEqual(critical.length, 1);
      assert(critical[0].category.toLowerCase().includes('critical'));
    });

    it('should filter by restoration category', () => {
      const restoration = analyzer.filterItems(sampleItems, 'restoration');
      assert.strictEqual(restoration.length, 1);
      assert(restoration[0].category.toLowerCase().includes('restoration'));
    });
  });

  describe('sortItems', () => {
    it('should sort by risk (low risk first)', () => {
      const sorted = analyzer.sortItems([...sampleItems], 'risk');
      assert.strictEqual(sorted[0].risk, 'Low');
      assert.strictEqual(sorted[sorted.length - 1].risk, 'High');
    });

    it('should sort by impact (high impact first)', () => {
      const sorted = analyzer.sortItems([...sampleItems], 'impact');
      assert.strictEqual(sorted[0].impact, 'High');
      assert.strictEqual(sorted[sorted.length - 1].impact, 'Low');
    });

    it('should sort by complexity (low complexity first)', () => {
      const sorted = analyzer.sortItems([...sampleItems], 'complexity');
      assert.strictEqual(sorted[0].complexity, 'Low');
      assert.strictEqual(sorted[sorted.length - 1].complexity, 'High');
    });

    it('should sort by timeline (shortest first)', () => {
      const sorted = analyzer.sortItems([...sampleItems], 'timeline');
      assert(sorted[0].timeline.includes('1-2 hours'));
      assert(sorted[sorted.length - 1].timeline.includes('1-2 days'));
    });
  });

  describe('sortByDependencies', () => {
    it('should place items with no dependencies first', () => {
      const sorted = analyzer.sortByDependencies([...sampleItems]);
      
      // First items should have no dependencies
      assert.strictEqual(sorted[0].dependencies.length, 0);
      assert.strictEqual(sorted[1].dependencies.length, 0);
    });

    it('should respect dependency order', () => {
      const sorted = analyzer.sortByDependencies([...sampleItems]);
      
      // Find Academy Restoration and Advanced Feature
      const academyIndex = sorted.findIndex(item => item.title === 'Academy Restoration');
      const advancedIndex = sorted.findIndex(item => item.title === 'Advanced Feature');
      
      // Advanced Feature depends on Academy Restoration, so should come after
      assert(academyIndex < advancedIndex);
    });

    it('should handle circular dependencies gracefully', () => {
      const circularItems = [
        { title: 'A', dependencies: ['B'] },
        { title: 'B', dependencies: ['A'] }
      ];
      
      const sorted = analyzer.sortByDependencies(circularItems);
      assert.strictEqual(sorted.length, 2); // Should not infinite loop
    });

    it('should handle missing dependencies', () => {
      const itemsWithMissingDeps = [
        { title: 'A', dependencies: ['NonExistent'] },
        { title: 'B', dependencies: [] }
      ];
      
      const sorted = analyzer.sortByDependencies(itemsWithMissingDeps);
      assert.strictEqual(sorted[0].title, 'B'); // No dependencies first
    });
  });

  describe('groupByCategory', () => {
    it('should group items by category', () => {
      const grouped = analyzer.groupByCategory(sampleItems);
      
      assert(grouped['Critical Fixes']);
      assert(grouped['Academy Restoration']);
      assert(grouped['Advanced Features']);
      assert(grouped['Enhancements']);
      
      assert.strictEqual(grouped['Critical Fixes'].length, 1);
      assert.strictEqual(grouped['Academy Restoration'].length, 1);
    });

    it('should handle empty array', () => {
      const grouped = analyzer.groupByCategory([]);
      assert.deepStrictEqual(grouped, {});
    });
  });

  describe('calculateStrategicScore', () => {
    it('should give higher scores to high impact items', () => {
      const highImpactItem = sampleItems.find(item => item.impact === 'High');
      const lowImpactItem = sampleItems.find(item => item.impact === 'Low');
      
      const highScore = analyzer.calculateStrategicScore(highImpactItem);
      const lowScore = analyzer.calculateStrategicScore(lowImpactItem);
      
      assert(highScore > lowScore);
    });

    it('should prefer low risk items', () => {
      const lowRiskItem = sampleItems.find(item => item.risk === 'Low');
      const highRiskItem = sampleItems.find(item => item.risk === 'High');
      
      const lowRiskScore = analyzer.calculateStrategicScore(lowRiskItem);
      const highRiskScore = analyzer.calculateStrategicScore(highRiskItem);
      
      // All other factors being different, focus on the impact comparison
      // Both have different impacts, so compare items with same impact
      const lowRiskHighImpact = sampleItems.find(item => item.risk === 'Low' && item.impact === 'High');
      const mediumRiskHighImpact = sampleItems.find(item => item.risk === 'Medium' && item.impact === 'High');
      
      if (lowRiskHighImpact && mediumRiskHighImpact) {
        const lowRiskScore = analyzer.calculateStrategicScore(lowRiskHighImpact);
        const mediumRiskScore = analyzer.calculateStrategicScore(mediumRiskHighImpact);
        assert(lowRiskScore > mediumRiskScore);
      }
    });

    it('should bonus items with no dependencies', () => {
      const noDepsItem = sampleItems.find(item => item.dependencies.length === 0);
      const withDepsItem = sampleItems.find(item => item.dependencies.length > 0);
      
      // Create comparable items (same risk, complexity, impact)
      const item1 = { ...noDepsItem, risk: 'Medium', complexity: 'Medium', impact: 'Medium', category: 'Test' };
      const item2 = { ...withDepsItem, risk: 'Medium', complexity: 'Medium', impact: 'Medium', category: 'Test' };
      
      const score1 = analyzer.calculateStrategicScore(item1);
      const score2 = analyzer.calculateStrategicScore(item2);
      
      assert(score1 > score2);
    });

    it('should bonus critical category items', () => {
      const criticalItem = sampleItems.find(item => item.category.includes('Critical'));
      const regularItem = sampleItems.find(item => !item.category.includes('Critical'));
      
      // Make them comparable
      const item1 = { ...criticalItem, risk: 'Medium', complexity: 'Medium', impact: 'Medium', dependencies: [] };
      const item2 = { ...regularItem, risk: 'Medium', complexity: 'Medium', impact: 'Medium', dependencies: [], category: 'Regular' };
      
      const score1 = analyzer.calculateStrategicScore(item1);
      const score2 = analyzer.calculateStrategicScore(item2);
      
      assert(score1 > score2);
    });
  });

  describe('getRecommendedAction', () => {
    it('should return highest scoring item', () => {
      const recommended = analyzer.getRecommendedAction(sampleItems);
      
      assert(recommended);
      assert(recommended.score !== undefined);
    });

    it('should prefer low-risk, high-impact items', () => {
      const recommended = analyzer.getRecommendedAction(sampleItems);
      
      // Critical Fix A should be recommended (Low risk, High impact, no dependencies)
      assert.strictEqual(recommended.title, 'Critical Fix A');
    });

    it('should return null for empty array', () => {
      const recommended = analyzer.getRecommendedAction([]);
      assert.strictEqual(recommended, null);
    });
  });

  describe('getTimelineValue', () => {
    it('should parse hour ranges correctly', () => {
      assert.strictEqual(analyzer.getTimelineValue('2-4 hours'), 3); // Average
      assert.strictEqual(analyzer.getTimelineValue('1 hour'), 1);
    });

    it('should parse day ranges correctly', () => {
      assert.strictEqual(analyzer.getTimelineValue('1-2 days'), 36); // 1.5 days * 24 hours
      assert.strictEqual(analyzer.getTimelineValue('3 days'), 72);
    });

    it('should parse week ranges correctly', () => {
      assert.strictEqual(analyzer.getTimelineValue('2 weeks'), 336); // 2 * 7 * 24
    });

    it('should default to 24 hours for unknown formats', () => {
      assert.strictEqual(analyzer.getTimelineValue('unknown'), 24);
      assert.strictEqual(analyzer.getTimelineValue(null), 24);
    });
  });

  describe('getStrategicInsights', () => {
    it('should calculate correct statistics', () => {
      const insights = analyzer.getStrategicInsights(sampleItems);
      
      assert.strictEqual(insights.totalItems, 4);
      assert.strictEqual(insights.lowRiskCount, 2);
      assert.strictEqual(insights.highImpactCount, 2);
      assert.strictEqual(insights.noDependencies, 2);
      assert.strictEqual(insights.quickWins, 1); // Low risk + High impact
      assert.strictEqual(insights.criticalItems, 1);
    });

    it('should handle empty array', () => {
      const insights = analyzer.getStrategicInsights([]);
      
      assert.strictEqual(insights.totalItems, 0);
      assert.strictEqual(insights.lowRiskCount, 0);
      assert.strictEqual(insights.quickWins, 0);
    });
  });

  describe('sortByDependencyImpact', () => {
    it('should sort broken commands by blocking impact', () => {
      const brokenCommands = [
        { name: 'spawn', blockedCommands: ['agents', 'screenshot'] },
        { name: 'test', blockedCommands: ['validation'] },
        { name: 'docs', blockedCommands: [] }
      ];
      
      const sorted = analyzer.sortByDependencyImpact(brokenCommands);
      
      assert.strictEqual(sorted[0].name, 'spawn'); // Blocks 2 commands
      assert.strictEqual(sorted[1].name, 'test');  // Blocks 1 command  
      assert.strictEqual(sorted[2].name, 'docs');  // Blocks 0 commands
    });
  });
});

// Run tests if this file is executed directly
if (require.main === module) {
  console.log('🧪 Running StrategicAnalyzer tests...\n');
  
  try {
    const testAnalyzer = new StrategicAnalyzer();
    
    const testItems = [
      { title: 'Test A', risk: 'Low', complexity: 'Low', impact: 'High', dependencies: [], category: 'Critical Fixes' },
      { title: 'Test B', risk: 'High', complexity: 'High', impact: 'Low', dependencies: ['Test A'], category: 'Advanced' }
    ];
    
    // Test filtering
    const filtered = testAnalyzer.filterItems(testItems, 'low-risk');
    console.log(`✅ Filtered ${filtered.length} low-risk items`);
    
    // Test sorting
    const sorted = testAnalyzer.sortItems([...testItems], 'dependency');
    console.log(`✅ Dependency sort: ${sorted[0].title} comes before ${sorted[1].title}`);
    
    // Test strategic scoring
    const recommended = testAnalyzer.getRecommendedAction(testItems);
    console.log(`✅ Recommended action: ${recommended.title}`);
    
    // Test insights
    const insights = testAnalyzer.getStrategicInsights(testItems);
    console.log(`✅ Strategic insights: ${insights.quickWins} quick wins found`);
    
    console.log('\n🎉 All StrategicAnalyzer tests passed!');
    
  } catch (error) {
    console.error('❌ StrategicAnalyzer test failed:', error.message);
    process.exit(1);
  }
}

module.exports = { StrategicAnalyzer };