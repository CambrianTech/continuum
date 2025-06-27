/**
 * Dead Code Analysis for WebSocketDaemon
 * Documents unused methods, dummy data, and cleanup opportunities
 */

export interface DeadCodeIssue {
  method: string;
  location: string;
  issue: 'unused' | 'dummy-data' | 'incomplete' | 'deprecated';
  description: string;
  recommendation: string;
}

export const WEBSOCKET_DAEMON_DEAD_CODE: DeadCodeIssue[] = [
  {
    method: 'proxyToRendererDaemon',
    location: 'src/integrations/websocket/WebSocketDaemon.ts:361',
    issue: 'unused',
    description: 'Method defined but never called. Intended for request forwarding.',
    recommendation: 'Remove or integrate into route handling system'
  },
  
  {
    method: 'requestFromDaemon',
    location: 'src/integrations/websocket/WebSocketDaemon.ts:535',
    issue: 'unused',
    description: 'Generic daemon request method that is never used.',
    recommendation: 'Remove or create proper daemon communication interface'
  },
  
  {
    method: 'getAgentsData',
    location: 'src/integrations/websocket/WebSocketDaemon.ts:475',
    issue: 'dummy-data',
    description: 'Returns hardcoded dummy agent data instead of real data.',
    recommendation: 'Connect to actual agent registry or remove'
  },
  
  {
    method: 'getPersonasData',
    location: 'src/integrations/websocket/WebSocketDaemon.ts:497',
    issue: 'dummy-data',
    description: 'Returns hardcoded dummy persona data instead of real data.',
    recommendation: 'Connect to actual persona system or remove'
  },
  
  {
    method: 'matchesPattern',
    location: 'src/integrations/websocket/WebSocketDaemon.ts:356',
    issue: 'incomplete',
    description: 'Simple regex pattern matching that could be more robust.',
    recommendation: 'Enhance or use established routing library'
  }
];

/**
 * Analyzes WebSocketDaemon for dead code and cleanup opportunities
 */
export class DeadCodeAnalyzer {
  
  static getUnusedMethods(): DeadCodeIssue[] {
    return WEBSOCKET_DAEMON_DEAD_CODE.filter(issue => issue.issue === 'unused');
  }
  
  static getDummyDataMethods(): DeadCodeIssue[] {
    return WEBSOCKET_DAEMON_DEAD_CODE.filter(issue => issue.issue === 'dummy-data');
  }
  
  static getIncompleteFeatures(): DeadCodeIssue[] {
    return WEBSOCKET_DAEMON_DEAD_CODE.filter(issue => issue.issue === 'incomplete');
  }
  
  static generateCleanupReport(): string {
    const unused = this.getUnusedMethods();
    const dummy = this.getDummyDataMethods();
    const incomplete = this.getIncompleteFeatures();
    
    return `
WebSocketDaemon Dead Code Analysis Report
========================================

Unused Methods (${unused.length}):
${unused.map(issue => `- ${issue.method}: ${issue.description}`).join('\n')}

Dummy Data Methods (${dummy.length}):
${dummy.map(issue => `- ${issue.method}: ${issue.description}`).join('\n')}

Incomplete Features (${incomplete.length}):
${incomplete.map(issue => `- ${issue.method}: ${issue.description}`).join('\n')}

Total Issues: ${WEBSOCKET_DAEMON_DEAD_CODE.length}

Recommendations:
1. Remove unused methods to reduce file size
2. Replace dummy data with real implementations or remove endpoints
3. Complete or remove incomplete features
4. Consider breaking large file into focused modules
`;
  }
}