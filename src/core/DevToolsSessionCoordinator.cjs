/**
 * DevToolsSessionCoordinator - Multi-Session Management
 * =====================================================
 * Coordinates DevTools sessions to prevent duplicate browser launches while
 * enabling proper multi-session support for different AI personas and purposes.
 * 
 * FEATURES:
 * - Prevents duplicate browser launches for same purpose
 * - Enables multiple AI persona sessions 
 * - Session isolation with dedicated artifacts
 * - Smart port allocation and browser reuse
 * - Session lifecycle management
 * - Emergency cleanup and recovery
 */

const fs = require('fs').promises;
const path = require('path');
const { spawn } = require('child_process');

class DevToolsSessionCoordinator {
    constructor() {
        this.activeSessions = new Map();
        this.portAllocation = new Map(); // port -> sessionId
        this.sessionArtifacts = new Map(); // sessionId -> artifactPath
        this.lockFile = '.continuum/devtools_coordinator.lock';
        this.sessionStore = '.continuum/devtools_sessions.json';
        
        // Default port range for DevTools
        this.portRange = {
            start: 9222,
            end: 9232  // Support up to 10 concurrent sessions
        };
        
        this.init();
    }

    async init() {
        // Ensure coordination directory exists
        await fs.mkdir('.continuum', { recursive: true });
        
        // Load existing sessions
        await this.loadSessions();
        
        // Clean up any stale sessions
        await this.cleanupStaleSessions();
    }

    /**
     * Request a DevTools session for specific purpose and AI persona
     */
    async requestSession(purpose, aiPersona = 'system', options = {}) {
        const sessionKey = this.generateSessionKey(purpose, aiPersona);
        
        // Check if session already exists for this purpose/persona combination
        const existingSession = this.activeSessions.get(sessionKey);
        if (existingSession && await this.isSessionActive(existingSession)) {
            console.log(`🔄 Reusing existing session: ${sessionKey}`);
            return existingSession;
        }

        // Create new session
        console.log(`🚀 Creating new DevTools session: ${sessionKey}`);
        const session = await this.createSession(purpose, aiPersona, options);
        
        // Store session
        this.activeSessions.set(sessionKey, session);
        await this.saveSessions();
        
        return session;
    }

    /**
     * Generate unique session key based on purpose and AI persona
     */
    generateSessionKey(purpose, aiPersona) {
        // Same purpose + same AI persona = same session (prevents duplicates)
        // Different AI personas = different sessions (enables multi-AI)
        return `${purpose}_${aiPersona}`;
    }

    /**
     * Create new DevTools session with browser launch
     */
    async createSession(purpose, aiPersona, options = {}) {
        const sessionId = `${purpose}_${aiPersona}_${Date.now()}`;
        const port = await this.allocatePort();
        
        if (!port) {
            throw new Error('No available ports for DevTools session');
        }

        const session = {
            sessionId: sessionId,
            purpose: purpose,
            aiPersona: aiPersona,
            port: port,
            created: new Date().toISOString(),
            status: 'starting',
            userDataDir: `/tmp/opera-devtools-${sessionId}`,
            options: options
        };

        try {
            // Launch browser with unique user data directory
            const browserProcess = await this.launchBrowser(session);
            session.browserPid = browserProcess.pid;
            session.status = 'active';
            
            // Wait for DevTools to be ready
            await this.waitForDevToolsReady(port);
            
            // Create session artifact if requested
            if (options.createArtifact !== false) {
                const artifactPath = await this.createSessionArtifact(session);
                session.artifactPath = artifactPath;
                this.sessionArtifacts.set(sessionId, artifactPath);
            }

            console.log(`✅ DevTools session active: ${sessionId} on port ${port}`);
            return session;

        } catch (error) {
            // Cleanup on failure
            await this.cleanupSession(sessionId);
            throw error;
        }
    }

    /**
     * Launch Opera browser for DevTools session
     */
    async launchBrowser(session) {
        const operaCmd = [
            '/Applications/Opera GX.app/Contents/MacOS/Opera',
            `--remote-debugging-port=${session.port}`,
            '--disable-web-security',
            '--disable-features=TranslateUI',
            '--disable-component-update',
            '--disable-background-timer-throttling',
            '--disable-backgrounding-occluded-windows',
            '--disable-renderer-backgrounding',
            '--no-first-run',
            '--no-default-browser-check',
            '--disable-default-apps',
            '--disable-extensions',
            `--user-data-dir=${session.userDataDir}`,
            'http://localhost:9000'
        ];

        console.log(`🌐 Launching Opera for session ${session.sessionId} on port ${session.port}`);
        const browserProcess = spawn(operaCmd[0], operaCmd.slice(1), {
            stdio: ['ignore', 'ignore', 'ignore'],
            detached: false
        });

        // Give browser time to start
        await new Promise(resolve => setTimeout(resolve, 2000));

        return browserProcess;
    }

    /**
     * Wait for DevTools to be ready on specified port
     */
    async waitForDevToolsReady(port, maxWait = 10000) {
        const startTime = Date.now();
        
        while (Date.now() - startTime < maxWait) {
            try {
                const response = await fetch(`http://localhost:${port}/json`);
                if (response.ok) {
                    const tabs = await response.json();
                    if (tabs.length > 0) {
                        console.log(`✅ DevTools ready on port ${port}`);
                        return true;
                    }
                }
            } catch (error) {
                // DevTools not ready yet, continue waiting
            }
            
            await new Promise(resolve => setTimeout(resolve, 500));
        }
        
        throw new Error(`DevTools not ready on port ${port} after ${maxWait}ms`);
    }

    /**
     * Allocate available port for new session
     */
    async allocatePort() {
        for (let port = this.portRange.start; port <= this.portRange.end; port++) {
            if (!this.portAllocation.has(port)) {
                // Check if port is actually available
                const isAvailable = await this.isPortAvailable(port);
                if (isAvailable) {
                    this.portAllocation.set(port, 'allocated');
                    return port;
                }
            }
        }
        return null;
    }

    /**
     * Check if port is available
     */
    async isPortAvailable(port) {
        try {
            const response = await fetch(`http://localhost:${port}/json`, {
                signal: AbortSignal.timeout(1000)
            });
            return false; // Port is in use
        } catch (error) {
            return true; // Port is available
        }
    }

    /**
     * Check if session is still active
     */
    async isSessionActive(session) {
        if (!session || !session.port) return false;
        
        try {
            const response = await fetch(`http://localhost:${session.port}/json`, {
                signal: AbortSignal.timeout(2000)
            });
            return response.ok;
        } catch (error) {
            return false;
        }
    }

    /**
     * Create WorkspaceArtifact for session if it's an AI persona session
     */
    async createSessionArtifact(session) {
        if (session.aiPersona === 'system') {
            // System sessions use VerificationArtifact
            return null; // Handled by git hook
        }

        // AI persona sessions get WorkspaceArtifact
        const WorkspaceArtifact = require('./artifacts/WorkspaceArtifact.cjs');
        const artifact = new WorkspaceArtifact(session.aiPersona, session.sessionId);
        
        await artifact.createStructure();
        await artifact.logWorkspaceEvent('session:started', {
            purpose: session.purpose,
            port: session.port,
            browserPid: session.browserPid
        });

        console.log(`📦 Session artifact created: ${artifact.artifactPath}`);
        return artifact.artifactPath;
    }

    /**
     * Close specific session
     */
    async closeSession(sessionKey) {
        const session = this.activeSessions.get(sessionKey);
        if (!session) return;

        console.log(`🔴 Closing session: ${sessionKey}`);
        
        await this.cleanupSession(session.sessionId);
        this.activeSessions.delete(sessionKey);
        await this.saveSessions();
    }

    /**
     * Cleanup session resources
     */
    async cleanupSession(sessionId) {
        const session = Array.from(this.activeSessions.values())
            .find(s => s.sessionId === sessionId);
        
        if (!session) return;

        // Kill browser process
        if (session.browserPid) {
            try {
                process.kill(session.browserPid, 'SIGTERM');
                console.log(`🔴 Browser process ${session.browserPid} terminated`);
            } catch (error) {
                // Process might already be dead
            }
        }

        // Free port
        if (session.port) {
            this.portAllocation.delete(session.port);
        }

        // Cleanup user data directory
        if (session.userDataDir) {
            try {
                await fs.rm(session.userDataDir, { recursive: true, force: true });
            } catch (error) {
                // Directory might not exist
            }
        }

        // Log session closure in artifact
        const artifactPath = this.sessionArtifacts.get(sessionId);
        if (artifactPath) {
            try {
                const WorkspaceArtifact = require('./artifacts/WorkspaceArtifact.cjs');
                // Note: would need session persistence to properly log closure
                console.log(`📦 Session artifact preserved: ${artifactPath}`);
            } catch (error) {
                // Artifact might not be accessible
            }
        }
    }

    /**
     * Cleanup stale sessions on startup
     */
    async cleanupStaleSessions() {
        console.log('🧹 Cleaning up stale DevTools sessions...');
        
        const staleKeys = [];
        for (const [sessionKey, session] of this.activeSessions.entries()) {
            const isActive = await this.isSessionActive(session);
            if (!isActive) {
                console.log(`🔴 Found stale session: ${sessionKey}`);
                staleKeys.push(sessionKey);
                await this.cleanupSession(session.sessionId);
            }
        }

        // Remove stale sessions
        for (const key of staleKeys) {
            this.activeSessions.delete(key);
        }

        if (staleKeys.length > 0) {
            await this.saveSessions();
            console.log(`✅ Cleaned up ${staleKeys.length} stale sessions`);
        }
    }

    /**
     * Load sessions from disk
     */
    async loadSessions() {
        try {
            const data = await fs.readFile(this.sessionStore, 'utf8');
            const sessions = JSON.parse(data);
            
            for (const [key, session] of Object.entries(sessions)) {
                this.activeSessions.set(key, session);
                if (session.port) {
                    this.portAllocation.set(session.port, session.sessionId);
                }
            }
            
            console.log(`📂 Loaded ${this.activeSessions.size} existing sessions`);
        } catch (error) {
            // No existing sessions file
            console.log('📂 No existing sessions found');
        }
    }

    /**
     * Save sessions to disk
     */
    async saveSessions() {
        const sessions = Object.fromEntries(this.activeSessions);
        await fs.writeFile(this.sessionStore, JSON.stringify(sessions, null, 2));
    }

    /**
     * Get session status summary
     */
    getSessionSummary() {
        const sessions = Array.from(this.activeSessions.values());
        return {
            totalSessions: sessions.length,
            byPurpose: sessions.reduce((acc, s) => {
                acc[s.purpose] = (acc[s.purpose] || 0) + 1;
                return acc;
            }, {}),
            byPersona: sessions.reduce((acc, s) => {
                acc[s.aiPersona] = (acc[s.aiPersona] || 0) + 1;
                return acc;
            }, {}),
            activePorts: Array.from(this.portAllocation.keys()),
            sessions: sessions.map(s => ({
                id: s.sessionId,
                purpose: s.purpose,
                persona: s.aiPersona,
                port: s.port,
                status: s.status
            }))
        };
    }

    /**
     * Emergency shutdown - close all sessions
     */
    async emergencyShutdown() {
        console.log('🚨 Emergency shutdown - closing all DevTools sessions...');
        
        for (const [sessionKey, session] of this.activeSessions.entries()) {
            await this.cleanupSession(session.sessionId);
        }
        
        this.activeSessions.clear();
        this.portAllocation.clear();
        
        // Clean up coordination files
        try {
            await fs.unlink(this.sessionStore);
            await fs.unlink(this.lockFile);
        } catch (error) {
            // Files might not exist
        }
        
        console.log('✅ Emergency shutdown complete');
    }
}

// Singleton instance
let coordinatorInstance = null;

/**
 * Get singleton DevTools session coordinator
 */
function getDevToolsCoordinator() {
    if (!coordinatorInstance) {
        coordinatorInstance = new DevToolsSessionCoordinator();
    }
    return coordinatorInstance;
}

module.exports = {
    DevToolsSessionCoordinator,
    getDevToolsCoordinator
};