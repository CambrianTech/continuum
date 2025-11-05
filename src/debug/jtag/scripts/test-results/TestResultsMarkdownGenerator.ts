/**
 * Test Results Markdown Generator
 * 
 * Generates comprehensive AI-friendly test result summaries with session tracking,
 * diagnostic information, and organized paths for debugging and analysis.
 */

import fs from 'fs/promises';
import path from 'path';
import { WorkingDirConfig } from '../../system/core/config/WorkingDirConfig';

export interface TestSession {
  sessionId: string;
  sessionPath: string;
  logFiles: string[];
  screenshotFiles: string[];
  createdAt?: Date;
  testName?: string;
  category?: string;
  testType?: 'CLI' | 'Browser' | 'System' | 'Unknown';
}

/**
 * Column configuration for table formatting
 */
interface TableColumn {
  key: string;
  header: string;
  minWidth: number;
  align?: 'left' | 'center' | 'right';
}

/**
 * Row data type for table formatting
 */
type TableRow = Record<string, string>;

/**
 * Table formatting utility for consistent column widths
 */
class MarkdownTableFormatter {
  private readonly columns: TableColumn[];
  private readonly padding = 1;

  constructor(columns: TableColumn[]) {
    this.columns = columns;
  }

  /**
   * Calculate optimal column widths based on content
   */
  private calculateColumnWidths(rows: TableRow[]): Map<string, number> {
    const widths = new Map<string, number>();
    
    // Initialize with header widths and minimum widths
    for (const col of this.columns) {
      widths.set(col.key, Math.max(col.header.length, col.minWidth));
    }
    
    // Calculate maximum content width for each column
    for (const row of rows) {
      for (const col of this.columns) {
        const content = row[col.key] || '';
        // Remove markdown formatting for accurate width calculation
        const visibleContent = content.replace(/\*\*|\`/g, '');
        const currentMax = widths.get(col.key) || col.minWidth;
        widths.set(col.key, Math.max(currentMax, visibleContent.length));
      }
    }
    
    return widths;
  }

  /**
   * Pad content to specified width with alignment
   */
  private padContent(content: string, width: number, align: 'left' | 'center' | 'right' = 'left'): string {
    // Calculate visible length (excluding markdown formatting)
    const visibleContent = content.replace(/\*\*|\`/g, '');
    const visibleLength = visibleContent.length;
    const padding = Math.max(0, width - visibleLength);
    
    switch (align) {
      case 'center': {
        const leftPad = Math.floor(padding / 2);
        const rightPad = padding - leftPad;
        return ' '.repeat(leftPad) + content + ' '.repeat(rightPad);
      }
      case 'right': {
        return ' '.repeat(padding) + content;
      }
      default: // 'left'
        return content + ' '.repeat(padding);
    }
  }

  /**
   * Generate properly formatted markdown table
   */
  formatTable(rows: TableRow[]): string[] {
    if (rows.length === 0) return [];
    
    const widths = this.calculateColumnWidths(rows);
    const lines: string[] = [];
    
    // Generate header row
    const headerCells = this.columns.map(col => {
      const width = widths.get(col.key) || col.minWidth;
      return this.padContent(col.header, width, 'left');
    });
    lines.push(`| ${headerCells.join(' | ')} |`);
    
    // Generate separator row with proper alignment
    const separatorCells = this.columns.map(col => {
      const width = widths.get(col.key) || col.minWidth;
      const align = col.align || 'left';
      
      switch (align) {
        case 'center':
          return ':' + '-'.repeat(Math.max(1, width - 2)) + ':';
        case 'right':
          return '-'.repeat(Math.max(1, width - 1)) + ':';
        default: // 'left'
          return '-'.repeat(width);
      }
    });
    lines.push(`| ${separatorCells.join(' | ')} |`);
    
    // Generate data rows
    for (const row of rows) {
      const dataCells = this.columns.map(col => {
        const width = widths.get(col.key) || col.minWidth;
        const content = row[col.key] || '';
        return this.padContent(content, width, col.align);
      });
      lines.push(`| ${dataCells.join(' | ')} |`);
    }
    
    return lines;
  }
}

export interface TestCategoryResult {
  category: string;
  passed: number;
  total: number;
  successRate: number;
  tests: string[];
  sessions: TestSession[];
}

export interface ComprehensiveTestResult {
  totalTests: number;
  passed: number;
  failed: number;
  successRate: number;
  categories: TestCategoryResult[];
  allSessions: TestSession[];
  systemInfo: {
    testStartTime: string;
    testEndTime: string;
    duration: string;
    workingDirectory: string;
    sessionRootPath: string;
    totalSessionsCreated: number;
  };
  diagnostics: {
    logFileCount: number;
    screenshotCount: number;
    totalLogSize: string;
    activeSessionsRemaining: number;
    expiredSessionsCleaned?: number;
  };
}

export class TestResultsMarkdownGenerator {
  private workingDir: string;
  private sessionRootPath: string;

  constructor() {
    this.workingDir = WorkingDirConfig.getWorkingDir();
    const continuumPath = WorkingDirConfig.getContinuumPath();
    this.sessionRootPath = path.join(continuumPath, 'jtag', 'sessions', 'user');
  }

  /**
   * Scan session directories and collect session information
   */
  async collectSessionInfo(): Promise<TestSession[]> {
    const sessions: TestSession[] = [];

    try {
      // Check if session root exists
      await fs.access(this.sessionRootPath);
      
      const sessionDirs = await fs.readdir(this.sessionRootPath);
      
      for (const sessionId of sessionDirs) {
        // Skip symlinks and non-UUID directories
        if (sessionId === 'currentUser' || !this.isUUID(sessionId)) {
          continue;
        }

        const sessionPath = path.join(this.sessionRootPath, sessionId);
        const stats = await fs.stat(sessionPath);
        
        if (stats.isDirectory()) {
          const session = await this.analyzeSession(sessionId, sessionPath, stats.birthtime);
          sessions.push(session);
        }
      }
    } catch (error) {
      console.warn(`Warning: Could not scan session directory ${this.sessionRootPath}:`, error);
    }

    return sessions.sort((a, b) => (b.createdAt?.getTime() || 0) - (a.createdAt?.getTime() || 0));
  }

  /**
   * Analyze individual session directory
   */
  private async analyzeSession(sessionId: string, sessionPath: string, createdAt: Date): Promise<TestSession> {
    const session: TestSession = {
      sessionId,
      sessionPath,
      logFiles: [],
      screenshotFiles: [],
      createdAt
    };

    try {
      // Check for logs directory
      const logsPath = path.join(sessionPath, 'logs');
      try {
        const logFiles = await fs.readdir(logsPath);
        session.logFiles = logFiles.map(f => path.join(logsPath, f));
      } catch {
        // No logs directory
      }

      // Check for screenshots directory
      const screenshotsPath = path.join(sessionPath, 'screenshots');
      try {
        const screenshotFiles = await fs.readdir(screenshotsPath);
        session.screenshotFiles = screenshotFiles.map(f => path.join(screenshotsPath, f));
      } catch {
        // No screenshots directory
      }

      // Infer test information from session characteristics
      await this.inferTestInfo(session);

    } catch (error) {
      console.warn(`Warning: Error analyzing session ${sessionId}:`, error);
    }

    return session;
  }

  /**
   * Infer test name and type from session characteristics
   */
  private async inferTestInfo(session: TestSession): Promise<void> {
    try {
      // Enhanced test type classification  
      const logFileNames = session.logFiles.map(f => f.toLowerCase());
      const hasServerLog = logFileNames.some(f => f.includes('server'));
      const hasBrowserLog = logFileNames.some(f => f.includes('browser'));
      const hasScreenshots = session.screenshotFiles.length > 0;

      if (hasServerLog && hasBrowserLog && hasScreenshots) {
        session.testType = 'Browser';
        session.category = 'Full-Stack Visual';
      } else if (hasServerLog && hasBrowserLog) {
        session.testType = 'System';
        session.category = 'Integration';
      } else if (hasServerLog && hasScreenshots) {
        session.testType = 'CLI';
        session.category = 'CLI Visual';
      } else if (hasServerLog) {
        session.testType = 'CLI';
        session.category = 'Server-side';
      } else if (hasBrowserLog) {
        session.testType = 'Browser';
        session.category = 'Client-side';
      } else {
        session.testType = 'Unknown';
        session.category = 'Empty';
      }

      // Try to extract test name from server log
      if (session.logFiles.length > 0) {
        const testName = await this.extractTestNameFromLogs(session);
        if (testName) {
          session.testName = testName;
        }
      }

      // If we found a test name, try to extract/enhance category
      if (session.testName) {
        const testNameLower = session.testName.toLowerCase();
        
        // Extract category from the test name for better organization
        if (testNameLower.includes('grid')) {
          session.category = 'Grid P2P';
        } else if (testNameLower.includes('transport')) {
          session.category = 'Transport Layer';
        } else if (testNameLower.includes('routing')) {
          session.category = 'Network Routing';
        } else if (testNameLower.includes('performance')) {
          session.category = 'Performance';
        } else if (testNameLower.includes('integration')) {
          session.category = 'Integration';
        } else if (testNameLower.includes('architecture')) {
          session.category = 'Architecture';
        } else if (testNameLower.includes('browser')) {
          session.category = 'Browser Automation';
        } else if (testNameLower.includes('system')) {
          session.category = 'System Level';
        } else if (testNameLower.includes('udp')) {
          session.category = 'UDP Transport';
        } else if (testNameLower.includes('websocket')) {
          session.category = 'WebSocket';
        } else if (testNameLower.includes('screenshot')) {
          session.category = 'Visual Testing';
        }
      }
      
      // If no test name found, generate descriptive name based on characteristics
      if (!session.testName) {
        session.testName = this.generateDescriptiveTestName(session);
      }

    } catch (error) {
      // Fallback to basic info
      session.testType = 'Unknown';
      session.testName = 'Unknown Test';
    }
  }

  /**
   * Extract test name from log files
   */
  private async extractTestNameFromLogs(session: TestSession): Promise<string | null> {
    try {
      // Look for server console log first - this has the most detailed info
      const serverLog = session.logFiles.find(f => f.includes('server-console-log.log')) ||
                       session.logFiles.find(f => f.includes('server.log'));
      if (serverLog) {
        const logContent = await fs.readFile(serverLog, 'utf-8');
        
        // Enhanced patterns to extract actual test names from JTAG execution
        const testPatterns = [
          // JTAG test execution patterns - look for .test.ts files being run (MOST RELIABLE)
          /npx tsx ([^\/\s]*\/)?([a-zA-Z0-9-_]+)\.test\.ts/,  // "npx tsx tests/grid-transport-foundation.test.ts"
          /timeout \d+s npx tsx ([^\/\s]*\/)?([a-zA-Z0-9-_]+)\.test\.ts/, // "timeout 30s npx tsx tests/test.ts"
          /▶️\s*([^\/\s]*\/)?([a-zA-Z0-9-_]+)\.test\.ts\s*\[([^\]]+)\]/, // "▶️ some-test.test.ts [Category]"
          /Running:\s+([^\/\s]*\/)?([a-zA-Z0-9-_]+)\.test\.ts/, // "Running: test-name.test.ts"
          /Testing\s+([^\/\s]*\/)?([a-zA-Z0-9-_]+)\.test\.ts/, // "Testing some-test.test.ts"
          /TEST:\s+([^\/\s]*\/)?([a-zA-Z0-9-_]+)\.test\.ts/, // "TEST: some-test.test.ts"
          
          // Look for test descriptions in logs (LESS RELIABLE - more restrictive)
          /▶️\s*([a-zA-Z][a-zA-Z0-9\s-_]{3,30})\s*\[([^\]]+)\]/, // "▶️ Simple Test Name [Category]" (no code chars)
          /Running:\s+([a-zA-Z][a-zA-Z0-9\s-_]{3,30})(?:\s|\n|$)/, // "Running: Simple Test Name" (no code chars) 
          /Testing\s+([a-zA-Z][a-zA-Z0-9\s-_]{3,30})(?:\s|\n|$)/, // "Testing simple feature" (no code chars)
          /TEST:\s+([a-zA-Z][a-zA-Z0-9\s-_]{3,30})(?:\s|\n|$)/, // "TEST: Simple test name" (no code chars)
          // Do NOT match console.log messages like: console.log('✅ WIDGET TEST: Modified chat widget state for visual comparison');
          // These patterns specifically exclude console.log context
        ];

        for (const pattern of testPatterns) {
          const match = logContent.match(pattern);
          if (match) {
            let testName = '';
            let category = '';
            
            // Handle different match groups based on pattern
            if (match[2]) {
              // Pattern with file path - use filename as test name
              testName = match[2];
              category = match[3] || '';
            } else if (match[1]) {
              testName = match[1].trim();
              category = match[2] || '';
            }
            
            if (testName) {
              // FIRST: Check if this looks like code/JavaScript - reject immediately
              const looksLikeCode = testName.includes('{') ||
                                   testName.includes('}') ||
                                   testName.includes('=>') ||
                                   testName.includes('function') ||
                                   testName.includes('console.log') ||
                                   testName.includes('return') ||
                                   testName.includes('\\n') ||
                                   testName.includes('\n') ||
                                   testName.includes('testName:') ||
                                   testName.includes('success:') ||
                                   testName.includes('shadowRoot') ||
                                   testName.includes('querySelector') ||
                                   testName.includes('getElementById') ||
                                   testName.includes('WIDGET TEST:') ||
                                   testName.includes('AUTOMATED') ||
                                   testName.length > 50; // Too long is likely code
              
              if (looksLikeCode) {
                continue; // Skip this match and try next pattern
              }
              
              // Clean up test name - convert kebab-case/snake_case to Title Case
              testName = testName
                .replace(/\.test$/, '')
                .split(/[_-]/)
                .filter(word => word.length > 0)  // Remove empty strings
                .map(word => word.charAt(0).toUpperCase() + word.slice(1))
                .join(' ');
              
              // Remove category brackets if present in testName
              testName = testName.replace(/\s*\[[^\]]+\]\s*$/, '');
              
              // Only return if we have a meaningful test name
              if (testName && testName.trim() && testName !== 'CLI Client') {
                // Add category as prefix if we found one
                if (category) {
                  return `${testName} [${category}]`;
                }
                
                return testName;
              }
            }
          }
        }

        // Look for screenshot command patterns
        const screenshotMatch = logContent.match(/screenshot.*--filename=([^\s]+)/);
        if (screenshotMatch && screenshotMatch[1]) {
          const filename = screenshotMatch[1].replace('.png', '').replace(/[_-]/g, ' ');
          return `Screenshot: ${filename}`;
        }

        // Look for command execution patterns  
        const commandMatch = logContent.match(/CommandDaemonServer.*commands\/([^/]+)/);
        if (commandMatch && commandMatch[1]) {
          const cmdName = commandMatch[1].replace(/[_-]/g, ' ');
          return `Command: ${cmdName}`;
        }
        
        // Look for CLI session display names - handle multiline JSON
        const displayNameMatch = logContent.match(/"displayName":\s*"([^"]+)"/);
        if (displayNameMatch && displayNameMatch[1] && displayNameMatch[1] !== 'CLI Client') {
          return displayNameMatch[1];
        }
        
        // Look for session creation patterns
        const sessionCreateMatch = logContent.match(/Creating session - "([^"]+)"/);
        if (sessionCreateMatch && sessionCreateMatch[1] && sessionCreateMatch[1] !== 'CLI Client') {
          return sessionCreateMatch[1];
        }
      }
    } catch (error) {
      // Log reading failed, continue without test name
    }
    
    return null;
  }

  /**
   * Generate descriptive test name based on session characteristics
   */
  private generateDescriptiveTestName(session: TestSession): string {
    const logCount = session.logFiles.length;
    const screenshotCount = session.screenshotFiles.length;
    const hasActivity = logCount > 0 || screenshotCount > 0;

    if (!hasActivity) {
      return 'Empty Session';
    }

    // Extract environment context from path
    let environment = '';
    if (session.sessionPath.includes('test-bench')) {
      environment = 'Test Bench';
    } else if (session.sessionPath.includes('widget-ui')) {
      environment = 'Widget UI';
    } else {
      environment = 'JTAG';
    }

    // Check for specific file patterns to infer test type
    const logFileNames = session.logFiles.map(f => f.toLowerCase());
    const hasServerLog = logFileNames.some(f => f.includes('server'));
    const hasBrowserLog = logFileNames.some(f => f.includes('browser'));
    
    // Generate more descriptive names based on characteristics
    if (screenshotCount > 0) {
      if (screenshotCount >= 8) {
        return `${environment}: Browser Automation Suite (${screenshotCount} captures)`;
      } else if (screenshotCount >= 3) {
        return `${environment}: Visual Integration Test (${screenshotCount} shots)`;
      } else {
        return `${environment}: UI Screenshot Test (${screenshotCount} capture${screenshotCount > 1 ? 's' : ''})`;
      }
    }

    // Categorize by log patterns and complexity
    if (hasServerLog && hasBrowserLog) {
      if (logCount >= 10) {
        return `${environment}: Full System Test Suite`;
      } else {
        return `${environment}: Cross-Context Integration`;
      }
    } else if (hasServerLog) {
      if (logCount >= 6) {
        return `${environment}: Server Integration Test`;
      } else {
        return `${environment}: Server Unit Test`;
      }
    } else if (hasBrowserLog) {
      return `${environment}: Browser Client Test`;
    }

    // Fallback based on log count with environment context
    if (logCount >= 10) {
      return `${environment}: Comprehensive Test`;
    } else if (logCount >= 6) {
      return `${environment}: Integration Test`;
    } else if (logCount >= 2) {
      return `${environment}: Unit Test`;
    } else {
      return `${environment}: Basic Test`;
    }
  }

  /**
   * Calculate diagnostics from session data
   */
  private async calculateDiagnostics(sessions: TestSession[]): Promise<ComprehensiveTestResult['diagnostics']> {
    let logFileCount = 0;
    let screenshotCount = 0;
    let totalLogSize = 0;

    for (const session of sessions) {
      logFileCount += session.logFiles.length;
      screenshotCount += session.screenshotFiles.length;

      // Calculate log file sizes
      for (const logFile of session.logFiles) {
        try {
          const stats = await fs.stat(logFile);
          totalLogSize += stats.size;
        } catch {
          // File may have been deleted
        }
      }
    }

    return {
      logFileCount,
      screenshotCount,
      totalLogSize: this.formatFileSize(totalLogSize),
      activeSessionsRemaining: sessions.length
    };
  }

  /**
   * Generate comprehensive markdown report
   */
  async generateMarkdownReport(testResult: Partial<ComprehensiveTestResult>): Promise<string> {
    const sessions = await this.collectSessionInfo();
    const diagnostics = await this.calculateDiagnostics(sessions);
    
    // Build comprehensive result
    const comprehensive: ComprehensiveTestResult = {
      totalTests: testResult.totalTests || 0,
      passed: testResult.passed || 0,
      failed: testResult.failed || 0,
      successRate: testResult.successRate || 0,
      categories: testResult.categories || [],
      allSessions: sessions,
      systemInfo: {
        testStartTime: testResult.systemInfo?.testStartTime || new Date().toISOString(),
        testEndTime: testResult.systemInfo?.testEndTime || new Date().toISOString(),
        duration: testResult.systemInfo?.duration || 'Unknown',
        workingDirectory: this.workingDir,
        sessionRootPath: this.sessionRootPath,
        totalSessionsCreated: sessions.length
      },
      diagnostics
    };

    return this.formatMarkdown(comprehensive);
  }

  /**
   * Format the comprehensive test result as markdown
   */
  private formatMarkdown(result: ComprehensiveTestResult): string {
    const sections: string[] = [];

    // Header - Human-friendly at a glance
    sections.push(`# JTAG Test Results`);
    sections.push(`**${result.successRate === 100 ? '🎉 ALL TESTS PASSED' : '⚠️ SOME TESTS FAILED'}** (${result.passed}/${result.totalTests})`);
    sections.push(``);
    sections.push(`**Generated**: ${new Date().toISOString()}`);
    sections.push(`**Working Directory**: \`${result.systemInfo.workingDirectory}\``);
    sections.push(`**Session Root**: \`${result.systemInfo.sessionRootPath}\``);
    sections.push(``);

    // Executive Summary
    sections.push(`## 📊 Executive Summary`);
    sections.push(``);
    sections.push(`| Metric | Value |`);
    sections.push(`|--------|-------|`);
    sections.push(`| **Total Tests** | ${result.totalTests} |`);
    sections.push(`| **Passed** | ✅ ${result.passed} |`);
    sections.push(`| **Failed** | ❌ ${result.failed} |`);
    sections.push(`| **Success Rate** | **${result.successRate}%** |`);
    sections.push(`| **Test Duration** | ${result.systemInfo.duration} |`);
    sections.push(`| **Sessions Created** | ${result.systemInfo.totalSessionsCreated} |`);
    sections.push(``);

    // Category Breakdown - Match original format exactly
    if (result.categories.length > 0) {
      sections.push(`📋 Results by Category:`);
      
      for (const categoryData of result.categories) {
        const passed = categoryData.passed;
        const total = categoryData.total;
        const successRate = categoryData.successRate;
        const failedCount = total - passed;
        
        // Format exactly like: "   Chat & Messaging:               3/ 4 tests ( 75%) ❌ 1 failed"
        const statusIcon = successRate === 100 ? '✅' : '❌';
        const statusText = successRate === 100 ? 'All passing' : `${failedCount} failed`;
        
        // Use proper spacing to align the output
        const categoryPadded = `   ${categoryData.category}:`.padEnd(35, ' ');
        const testsPadded = `${passed}/ ${total} tests`.padEnd(12, ' ');
        const ratePadded = `(${successRate.toString().padStart(3, ' ')}%)`;
        
        sections.push(`${categoryPadded}${testsPadded} ${ratePadded} ${statusIcon} ${statusText}`);
      }
      
      sections.push(``);
      sections.push(`❌ DETAILED FAILURE BREAKDOWN:`);
      sections.push(`🔍 Recommended Next Steps:`);
      sections.push(`   1. Run specific failing tests with detailed output`);
      sections.push(`   2. Check system logs: .continuum/jtag/system/logs/`);
      sections.push(`   3. Verify system health: npm run agent:quick`);
      sections.push(`   4. Use profile-specific commands for focused testing`);
      sections.push(``);
    }

    // Session Analysis - Key for AI Debugging
    sections.push(`## 🔍 Session Analysis (AI Debugging Guide)`);
    sections.push(``);
    sections.push(`**Session Root**: \`${result.systemInfo.sessionRootPath}\``);
    sections.push(``);

    if (result.allSessions.length > 0) {
      sections.push(`### 📁 Session Directories`);
      sections.push(``);
      
      // Recent sessions first (most relevant for debugging)
      const recentSessions = result.allSessions.slice(0, 10);
      
      // Create clean, scannable format optimized for humans and AIs
      for (const session of recentSessions) {
        const createdTime = session.createdAt ? session.createdAt.toLocaleTimeString() : 'Unknown';
        const logCount = session.logFiles.length;
        const screenshotCount = session.screenshotFiles.length;
        const shortPath = session.sessionPath.replace(result.systemInfo.workingDirectory, '.');
        
        // Clean up test names - remove code blocks and verbose content
        let testName = session.testName || 'Unknown Test';
        
        // Aggressively filter out code/JavaScript content
        const isCodeContent = (name: string) => {
          return name.length > 100 ||
                 name.includes('function') ||
                 name.includes('{') ||
                 name.includes('}') ||
                 name.includes('=>') ||
                 name.includes('console.log') ||
                 name.includes('return {') ||
                 name.includes('\\n') ||
                 name.includes('\n') ||
                 name.includes('testName:') ||
                 name.includes('success:') ||
                 name.includes('error:');
        };
        
        if (isCodeContent(testName)) {
          // Extract meaningful name from patterns in verbose content
          if (testName.includes('widgetStateModification')) {
            testName = 'Widget State Modification Test';
          } else if (testName.includes('visualTest') || testName.includes('visual comparison')) {
            testName = 'Visual Comparison Test';
          } else if (testName.includes('screenshot')) {
            testName = 'Screenshot Test';
          } else if (testName.includes('browser')) {
            testName = 'Browser Automation Test';
          } else if (testName.includes('AUTOMATED') && testName.includes('TEST')) {
            testName = 'Automated UI Test';
          } else {
            // Fallback - use descriptive name based on session characteristics
            testName = this.generateDescriptiveTestName(session);
          }
        }
        
        const testType = session.testType || 'Unknown';
        const category = session.category || 'General';
        
        // Create status indicator based on files
        let status = '📋 Session';
        if (screenshotCount > 0 && logCount > 5) {
          status = '✅ Visual Test Complete';
        } else if (screenshotCount > 0) {
          status = '📸 Screenshots Taken';
        } else if (logCount > 0) {
          status = '📝 Logs Generated';
        } else {
          status = '❓ Empty Session';
        }
        
        sections.push(`### ${status}: **${testName}**`);
        sections.push(`**${category} • ${testType} • ${createdTime}**`);
        sections.push(`📊 **Results**: ${logCount} logs${screenshotCount > 0 ? `, ${screenshotCount} screenshots` : ''}`);
        sections.push(`🔗 **Session**: \`${session.sessionId.slice(0, 12)}...\``);
        sections.push(`📁 **Location**: \`${shortPath}\``);
        sections.push(``);
      }
      
      if (result.allSessions.length > 10) {
        sections.push(`### ... *${result.allSessions.length - 10} more sessions*`);
        sections.push(``);
      }
      sections.push(``);

      // Quick Access Commands
      sections.push(`### 🚀 Quick Access Commands (Copy-Paste Ready)`);
      sections.push(``);
      sections.push(`**View most recent session logs:**`);
      sections.push(`\`\`\`bash`);
      if (recentSessions.length > 0) {
        const latest = recentSessions[0];
        const testName = latest.testName || 'Unknown Test';
        sections.push(`# Latest session: ${testName} (${latest.sessionId.slice(0, 8)}...)`);
        sections.push(`cd "${latest.sessionPath}"`);
        if (latest.logFiles.length > 0) {
          sections.push(`tail -50 logs/server.log`);
          sections.push(`tail -50 logs/browser.log`);
        }
        if (latest.screenshotFiles.length > 0) {
          sections.push(`open screenshots/  # View screenshots`);
        }
      }
      sections.push(`\`\`\``);
      sections.push(``);

      sections.push(`**Access current session (symlink):**`);
      sections.push(`\`\`\`bash`);
      sections.push(`cd "${path.join(result.systemInfo.sessionRootPath, '../currentUser')}"`);
      sections.push(`ls -la logs/`);
      sections.push(`ls -la screenshots/`);
      sections.push(`\`\`\``);
      sections.push(``);
    }

    // Diagnostics
    sections.push(`## 🔧 System Diagnostics`);
    sections.push(``);
    sections.push(`| Metric | Value |`);
    sections.push(`|--------|-------|`);
    sections.push(`| **Log Files Created** | ${result.diagnostics.logFileCount} |`);
    sections.push(`| **Screenshots Taken** | ${result.diagnostics.screenshotCount} |`);
    sections.push(`| **Total Log Data** | ${result.diagnostics.totalLogSize} |`);
    sections.push(`| **Active Sessions** | ${result.diagnostics.activeSessionsRemaining} |`);
    sections.push(``);

    // Human Audience: Quick Status
    sections.push(`## 👥 For Humans: Quick Status`);
    sections.push(``);
    if (result.successRate === 100) {
      sections.push(`🎉 **Great news!** All tests are passing. The system is working correctly.`);
      sections.push(``);
      sections.push(`**What this means:**`);
      sections.push(`- All ${result.totalTests} tests completed successfully`);
      sections.push(`- ${result.systemInfo.totalSessionsCreated} test sessions created (proper isolation)`);
      sections.push(`- System is ready for development and deployment`);
    } else {
      sections.push(`⚠️ **Attention needed:** ${result.failed} tests are failing.`);
      sections.push(``);
      sections.push(`**Priority actions:**`);
      sections.push(`1. Check failed test categories in the table above`);
      sections.push(`2. Use session paths below to examine specific test logs`);
      sections.push(`3. Look at screenshots for visual test failures`);
    }
    sections.push(``);

    // AI Audience: Structured Information
    sections.push(`## 🤖 For AIs: Debugging Data`);
    sections.push(``);
    
    sections.push(`### Architecture Overview`);
    sections.push(`- **Session Isolation**: Each test gets unique session directory with isolated logs/screenshots`);
    sections.push(`- **Expiry Management**: CLI sessions (30min), Browser sessions (24hr), Auto-cleanup every 5min`);
    sections.push(`- **Activity Tracking**: Session usage extends expiry timers automatically`);
    sections.push(`- **Concurrent Safety**: Multiple tests can run simultaneously without conflicts`);
    sections.push(``);

    sections.push(`### Session Debugging Protocol`);
    sections.push(`1. **Test Failure Analysis**: Identify category and specific test name from results above`);
    sections.push(`2. **Session Identification**: Use session table to locate relevant session UUID`);
    sections.push(`3. **Log Examination**: Navigate to session path and examine server.log + browser.log`);
    sections.push(`4. **Visual Validation**: Check screenshots directory for captured test states`);
    sections.push(`5. **Cross-Session Comparison**: Compare successful vs failed session logs`);
    sections.push(``);

    sections.push(`### Structured Session Data (JSON-like)`);
    sections.push(`\`\`\`json`);
    sections.push(`{`);
    sections.push(`  "sessionRoot": "${result.systemInfo.sessionRootPath}",`);
    sections.push(`  "totalSessions": ${result.systemInfo.totalSessionsCreated},`);
    sections.push(`  "currentSymlink": "${path.join(result.systemInfo.sessionRootPath, '../currentUser')}",`);
    if (result.allSessions.length > 0) {
      const latest = result.allSessions[0];
      sections.push(`  "latestSession": {`);
      sections.push(`    "id": "${latest.sessionId}",`);
      sections.push(`    "path": "${latest.sessionPath}",`);
      sections.push(`    "logFiles": ${latest.logFiles.length},`);
      sections.push(`    "screenshots": ${latest.screenshotFiles.length}`);
      sections.push(`  },`);
    }
    sections.push(`  "diagnostics": {`);
    sections.push(`    "totalLogFiles": ${result.diagnostics.logFileCount},`);
    sections.push(`    "totalScreenshots": ${result.diagnostics.screenshotCount},`);
    sections.push(`    "logDataSize": "${result.diagnostics.totalLogSize}"`);
    sections.push(`  }`);
    sections.push(`}`);
    sections.push(`\`\`\``);
    sections.push(``);

    // Footer
    sections.push(`---`);
    sections.push(`**Report generated by**: JTAG Test Results Markdown Generator`);
    sections.push(`**Session architecture**: Individual session isolation with automatic expiry`);
    sections.push(`**Next steps**: Use session paths above for detailed debugging`);

    return sections.join('\n');
  }

  /**
   * Utility methods
   */
  private isUUID(str: string): boolean {
    const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
    return uuidRegex.test(str);
  }

  private formatFileSize(bytes: number): string {
    const sizes = ['B', 'KB', 'MB', 'GB'];
    if (bytes === 0) return '0 B';
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${sizes[i]}`;
  }
}

/**
 * Generate and save test results markdown
 */
export async function generateTestResultsMarkdown(
  testResult: Partial<ComprehensiveTestResult>,
  outputPath?: string
): Promise<string> {
  const generator = new TestResultsMarkdownGenerator();
  const markdown = await generator.generateMarkdownReport(testResult);
  
  if (outputPath) {
    // Ensure directory exists
    await fs.mkdir(path.dirname(outputPath), { recursive: true });
    await fs.writeFile(outputPath, markdown, 'utf-8');
    console.log(`📄 Test results saved to: ${outputPath}`);
  }
  
  return markdown;
}