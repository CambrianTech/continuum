/**
 * StrategicAnalyzer - Filter, sort, and analyze roadmap items strategically
 * Provides dependency-aware prioritization and risk analysis
 */

class StrategicAnalyzer {
  constructor() {
    // Risk/complexity/impact value mappings
    this.riskValues = { 'Low': 1, 'Medium': 2, 'High': 3 };
    this.complexityValues = { 'Low': 1, 'Medium': 2, 'High': 3 };
    this.impactValues = { 'Low': 1, 'Medium': 2, 'High': 3 };
  }

  /**
   * Filter roadmap items based on criteria
   */
  filterItems(items, filter) {
    if (filter === 'all') return items;
    
    switch (filter) {
      case 'risk':
        return items.filter(item => item.risk === 'High');
      case 'low-risk':
        return items.filter(item => item.risk === 'Low');
      case 'complexity':
        return items.filter(item => item.complexity === 'Low');
      case 'impact':
        return items.filter(item => item.impact === 'High');
      case 'critical':
        return items.filter(item => item.category.toLowerCase().includes('critical'));
      case 'restoration':
        return items.filter(item => item.category.toLowerCase().includes('restoration'));
      default:
        return items;
    }
  }

  /**
   * Sort roadmap items based on strategy
   */
  sortItems(items, sort) {
    switch (sort) {
      case 'dependency':
        return this.sortByDependencies(items);
      case 'risk':
        return items.sort((a, b) => this.riskValues[a.risk] - this.riskValues[b.risk]);
      case 'impact':
        return items.sort((a, b) => this.impactValues[b.impact] - this.impactValues[a.impact]);
      case 'complexity':
        return items.sort((a, b) => this.complexityValues[a.complexity] - this.complexityValues[b.complexity]);
      case 'timeline':
        return items.sort((a, b) => this.getTimelineValue(a.timeline) - this.getTimelineValue(b.timeline));
      default:
        return items;
    }
  }

  /**
   * Topological sort by dependencies - safe execution order
   */
  sortByDependencies(items) {
    const sorted = [];
    const remaining = [...items];
    
    // Safety valve for infinite loops
    let maxIterations = items.length * 2;
    let iterations = 0;
    
    while (remaining.length > 0 && iterations < maxIterations) {
      iterations++;
      
      const nextItem = remaining.find(item => 
        !item.dependencies || 
        item.dependencies.length === 0 ||
        item.dependencies.every(dep => 
          sorted.some(sortedItem => sortedItem.title.toLowerCase().includes(dep.toLowerCase()))
        )
      );
      
      if (nextItem) {
        sorted.push(nextItem);
        remaining.splice(remaining.indexOf(nextItem), 1);
      } else {
        // Circular dependency or missing dependency - add remaining items
        sorted.push(...remaining);
        break;
      }
    }
    
    return sorted;
  }

  /**
   * Group items by category for organized display
   */
  groupByCategory(items) {
    const groups = {};
    items.forEach(item => {
      if (!groups[item.category]) {
        groups[item.category] = [];
      }
      groups[item.category].push(item);
    });
    return groups;
  }

  /**
   * Analyze dependency impact for broken commands
   */
  sortByDependencyImpact(brokenCommands) {
    return brokenCommands.sort((a, b) => {
      const aBlocks = (a.blockedCommands || []).length;
      const bBlocks = (b.blockedCommands || []).length;
      return bBlocks - aBlocks; // Higher blocking count first
    });
  }

  /**
   * Get next recommended action based on strategic criteria
   */
  getRecommendedAction(items) {
    if (items.length === 0) return null;

    // Prefer items with: Low risk + High impact + No dependencies
    const scored = items.map(item => ({
      ...item,
      score: this.calculateStrategicScore(item)
    }));

    scored.sort((a, b) => b.score - a.score);
    return scored[0];
  }

  /**
   * Calculate strategic score for prioritization
   */
  calculateStrategicScore(item) {
    let score = 0;
    
    // Impact scoring (most important)
    score += this.impactValues[item.impact] * 3;
    
    // Risk scoring (inverse - lower risk is better)
    score += (4 - this.riskValues[item.risk]) * 2;
    
    // Complexity scoring (inverse - lower complexity is better)  
    score += (4 - this.complexityValues[item.complexity]) * 1;
    
    // Dependency bonus (no dependencies = easier to start)
    if (!item.dependencies || item.dependencies.length === 0) {
      score += 2;
    }
    
    // Critical category bonus
    if (item.category.toLowerCase().includes('critical')) {
      score += 3;
    }
    
    return score;
  }

  /**
   * Convert timeline string to hours for sorting
   */
  getTimelineValue(timeline) {
    if (!timeline) return 24; // Default to 1 day
    
    if (timeline.includes('hour')) {
      const hours = parseInt(timeline.match(/(\d+)/)?.[1] || '4');
      return hours;
    }
    if (timeline.includes('day')) {
      const days = parseInt(timeline.match(/(\d+)/)?.[1] || '3');
      return days * 24;
    }
    if (timeline.includes('week')) {
      const weeks = parseInt(timeline.match(/(\d+)/)?.[1] || '2');
      return weeks * 7 * 24;
    }
    
    return 24; // Default to 1 day
  }

  /**
   * Get strategic insights for items
   */
  getStrategicInsights(items) {
    const insights = {
      totalItems: items.length,
      lowRiskCount: items.filter(i => i.risk === 'Low').length,
      highImpactCount: items.filter(i => i.impact === 'High').length,
      noDependencies: items.filter(i => !i.dependencies || i.dependencies.length === 0).length,
      quickWins: items.filter(i => i.risk === 'Low' && i.impact === 'High').length,
      criticalItems: items.filter(i => i.category.toLowerCase().includes('critical')).length
    };

    return insights;
  }
}

module.exports = StrategicAnalyzer;