/**
 * JTAG Session Integration - Organized artifact storage for debugging
 * 
 * Extends SessionManager to store JTAG unit outputs (screenshots, console logs, 
 * JS execution results) in organized session directories for easy access by:
 * - Portal (current user)
 * - Web console (animated display)  
 * - Personas/agents (debugging access)
 * - Anyone in Continuum (unified discovery)
 */

const SessionManager = require('./SessionManager.cjs');
const path = require('path');
const fs = require('fs').promises;

class JTAGSessionIntegration extends SessionManager {
    constructor(baseDir = '.') {
        super(baseDir);
        this.currentSession = null; // Track active JTAG session
    }

    /**
     * Start a JTAG debugging session 
     * @param {string} runId - Session identifier
     * @param {Object} metadata - Session metadata (user, purpose, etc.)
     */
    async startJTAGSession(runId, metadata = {}) {
        try {
            const sessionPath = await this.createSession('portal', runId, {
                ...metadata,
                sessionType: 'jtag',
                jtagCapabilities: ['screenshots', 'console-logs', 'js-execution', 'server-logs']
            });
            
            this.currentSession = {
                type: 'portal',
                runId,
                path: sessionPath
            };
            
            // Initialize JTAG artifact structure
            await this.initializeJTAGArtifacts(sessionPath);
            
            console.log(`🔬 JTAG session started: portal/run_${runId}`);
            return sessionPath;
            
        } catch (error) {
            console.error('❌ JTAG session start failed:', error);
            throw error;
        }
    }

    /**
     * Initialize JTAG-specific artifact files
     */
    async initializeJTAGArtifacts(sessionPath) {
        try {
            // Create empty artifact files with headers
            await fs.writeFile(
                path.join(sessionPath, 'console-logs.txt'),
                `# Browser Console Logs - JTAG Session\n# Timestamp: ${new Date().toISOString()}\n\n`
            );
            
            await fs.writeFile(
                path.join(sessionPath, 'js-execution.txt'),
                `# JavaScript Execution Results - JTAG Session\n# Timestamp: ${new Date().toISOString()}\n\n`
            );
            
            await fs.writeFile(
                path.join(sessionPath, 'server-logs.txt'),
                `# Server/Daemon Logs - JTAG Session\n# Timestamp: ${new Date().toISOString()}\n\n`
            );
            
            // Create screenshots subdirectory
            await fs.mkdir(path.join(sessionPath, 'screenshots'), { recursive: true });
            
            console.log('📁 JTAG artifacts initialized');
            
        } catch (error) {
            console.error('⚠️ JTAG artifact initialization failed:', error);
        }
    }

    /**
     * Store screenshot in current JTAG session
     * @param {string} screenshotPath - Source screenshot path
     * @param {string} filename - Optional custom filename
     */
    async storeScreenshot(screenshotPath, filename = null) {
        if (!this.currentSession) {
            console.log('⚠️ No active JTAG session - screenshot stored globally');
            return screenshotPath;
        }
        
        try {
            const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
            const screenshotName = filename || `screenshot_${timestamp}.png`;
            const sessionScreenshotPath = path.join(
                this.currentSession.path, 
                'screenshots', 
                screenshotName
            );
            
            await fs.copyFile(screenshotPath, sessionScreenshotPath);
            
            // Log the screenshot activity
            await this.addLogEntry(
                this.currentSession.type,
                this.currentSession.runId,
                'console-logs',
                `📸 Screenshot captured: ${screenshotName}`
            );
            
            console.log(`📸 Screenshot stored in session: ${screenshotName}`);
            return sessionScreenshotPath;
            
        } catch (error) {
            console.error('⚠️ Failed to store screenshot in session:', error);
            return screenshotPath; // Return original path as fallback
        }
    }

    /**
     * Store JavaScript execution result in current session
     * @param {string} script - JavaScript code executed
     * @param {*} result - Execution result
     * @param {Array} consoleOutput - Console messages captured
     */
    async storeJSExecution(script, result, consoleOutput = []) {
        if (!this.currentSession) return;
        
        try {
            const timestamp = new Date().toISOString();
            const executionEntry = `
[${timestamp}] JavaScript Execution
================================
Script:
${script}

Result:
${JSON.stringify(result, null, 2)}

Console Output (${consoleOutput.length} messages):
${consoleOutput.map(msg => `  ${msg}`).join('\n')}

--------------------------------
`;
            
            await this.writeToArtifact(
                this.currentSession.type,
                this.currentSession.runId,
                'js-execution',
                executionEntry,
                true // append
            );
            
            // Also log to console logs
            for (const msg of consoleOutput) {
                await this.addLogEntry(
                    this.currentSession.type,
                    this.currentSession.runId,
                    'console-logs',
                    `🟢 CONSOLE: ${msg}`
                );
            }
            
            console.log('🔬 JS execution stored in session');
            
        } catch (error) {
            console.error('⚠️ Failed to store JS execution:', error);
        }
    }

    /**
     * Store server/daemon logs in current session
     * @param {string} logContent - Log content to store
     * @param {string} source - Source of logs (daemon, server, etc.)
     */
    async storeServerLogs(logContent, source = 'server') {
        if (!this.currentSession) return;
        
        try {
            const timestamp = new Date().toISOString();
            const logEntry = `
[${timestamp}] ${source.toUpperCase()} LOGS
================================
${logContent}
--------------------------------
`;
            
            await this.writeToArtifact(
                this.currentSession.type,
                this.currentSession.runId,
                'server-logs',
                logEntry,
                true // append
            );
            
            console.log(`🖥️ ${source} logs stored in session`);
            
        } catch (error) {
            console.error('⚠️ Failed to store server logs:', error);
        }
    }

    /**
     * Complete current JTAG session with summary
     * @param {Object} results - Session results and summary
     */
    async completeJTAGSession(results = {}) {
        if (!this.currentSession) return;
        
        try {
            await this.completeSession(
                this.currentSession.type,
                this.currentSession.runId,
                {
                    ...results,
                    jtagCapabilities: ['screenshots', 'console-logs', 'js-execution', 'server-logs'],
                    summary: results.summary || 'JTAG debugging session completed'
                }
            );
            
            console.log(`✅ JTAG session completed: ${this.currentSession.runId}`);
            this.currentSession = null;
            
        } catch (error) {
            console.error('❌ JTAG session completion failed:', error);
        }
    }

    /**
     * Get current session path for external tools
     */
    getCurrentSessionPath() {
        return this.currentSession?.path || null;
    }

    /**
     * Quick helper to get artifacts from any JTAG session
     * @param {string} runId - Session run ID (or 'latest')
     */
    async getJTAGArtifacts(runId = 'latest') {
        try {
            const artifacts = {};
            
            // Read all JTAG artifacts
            artifacts.consoleLog = await this.readArtifact('portal', runId, 'console-logs');
            artifacts.jsExecution = await this.readArtifact('portal', runId, 'js-execution');
            artifacts.serverLogs = await this.readArtifact('portal', runId, 'server-logs');
            
            // List screenshots
            const sessionPath = runId === 'latest' 
                ? await this.getLatestSession('portal')
                : path.join(this.sessionsDir, 'portal', `run_${runId}`);
                
            const screenshotsDir = path.join(sessionPath, 'screenshots');
            try {
                artifacts.screenshots = await fs.readdir(screenshotsDir);
            } catch (e) {
                artifacts.screenshots = [];
            }
            
            return artifacts;
            
        } catch (error) {
            console.error('⚠️ Failed to get JTAG artifacts:', error);
            return null;
        }
    }
}

module.exports = JTAGSessionIntegration;