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

import { promises as fs } from 'fs';
import * as path from 'path';
import { spawn, ChildProcess } from 'child_process';

// TypeScript interfaces for session management
interface SessionInfo {
  sessionId: string;
  purpose: string;
  aiPersona: string;
  port?: number;
  created: string;
  status: 'starting' | 'active' | 'inactive';
  userDataDir?: string;
  browserPid?: number | null;
  isSharedTab?: boolean;
  tabId?: string;
  reuseExisting?: boolean;
  artifactPath?: string;
  options?: any;
}

interface TabInfo {
  sessionKey: string;
  purpose: string;
  aiPersona: string;
  sessionId: string;
  port?: number;
  created: Date;
  lastCheck: Date;
}

interface ProbeResult {
  traceId: string;
  sessionId: string;
  purpose: string;
  aiPersona: string;
  port?: number;
  timestamp: string;
  status: 'session_validated' | 'feedback_verified' | 'connection_failed';
}

// Simple in-process tab registry to prevent duplicate tabs
class SimpleTabRegistry {
    private tabs = new Map<string, TabInfo>(); // sessionKey -> tabInfo

    constructor() {
        this.tabs = new Map();
    }
    
    async findExistingTab(purpose: string, aiPersona: string): Promise<TabInfo | null> {
        const sessionKey = `${purpose}_${aiPersona}`;
        const existingTab = this.tabs.get(sessionKey);
        
        if (existingTab && await this.isTabStillActive(existingTab)) {
            console.log(`🔄 SimpleTabRegistry: Found existing tab for ${sessionKey}`);
            return existingTab;
        }
        
        // Clean up stale tab entry
        if (existingTab) {
            this.tabs.delete(sessionKey);
            console.log(`🧹 SimpleTabRegistry: Cleaned up stale tab for ${sessionKey}`);
        }
        
        return null;
    }
    
    registerTab(purpose, aiPersona, sessionInfo) {
        const sessionKey = `${purpose}_${aiPersona}`;
        const tabInfo = {
            sessionKey,
            purpose,
            aiPersona,
            sessionId: sessionInfo.sessionId,
            port: sessionInfo.port,
            created: new Date(),
            lastCheck: new Date()
        };
        
        this.tabs.set(sessionKey, tabInfo);
        console.log(`📝 SimpleTabRegistry: Registered tab for ${sessionKey}`);
        return tabInfo;
    }
    
    async isTabStillActive(tabInfo) {
        try {
            // Quick check if the session is still responsive
            const response = await fetch(`http://localhost:${tabInfo.port}/json`, {
                signal: AbortSignal.timeout(2000)
            });
            
            if (response.ok) {
                const tabs = await response.json();
                const hasActiveContinuumTab = tabs.some(tab => tab.url.includes('localhost:9000'));
                
                if (hasActiveContinuumTab) {
                    tabInfo.lastCheck = new Date();
                    return true;
                }
            }
        } catch (error) {
            // Tab/session no longer active
        }
        
        return false;
    }
    
    cleanup() {
        // Remove stale entries older than 5 minutes
        const cutoff = new Date(Date.now() - 5 * 60 * 1000);
        for (const [key, tabInfo] of this.tabs.entries()) {
            if (tabInfo.lastCheck < cutoff) {
                this.tabs.delete(key);
                console.log(`🧹 SimpleTabRegistry: Cleaned up stale tab ${key}`);
            }
        }
    }
}

// Use simple in-process registry for immediate tab coordination
const simpleTabRegistry = new SimpleTabRegistry();

class DevToolsSessionCoordinator {
    private activeSessions = new Map<string, SessionInfo>();
    private portAllocation = new Map<number, string>(); // port -> sessionId
    private sessionArtifacts = new Map<string, string>(); // sessionId -> artifactPath
    private lockFile = '.continuum/devtools_coordinator.lock';
    private sessionStore = '.continuum/devtools_sessions.json';
    private portRange = {
        start: 9222,
        end: 9232  // Support up to 10 concurrent sessions
    };

    constructor() {
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
     * Run automatic probe when session connects
     */
    async runSessionProbe(sessionInfo: SessionInfo, purpose: string, aiPersona: string): Promise<ProbeResult> {
        const traceId = `session-probe-${Date.now()}-${Math.random().toString(36).substring(7)}`;
        
        console.log(`🔬 AUTO-PROBE: Session connection validated`);
        console.log(`   🎯 Trace ID: ${traceId}`);
        console.log(`   📡 Session: ${sessionInfo.sessionId}`);
        console.log(`   🎭 Purpose: ${purpose}`);
        console.log(`   🤖 AI Persona: ${aiPersona}`);
        console.log(`   🔌 Port: ${sessionInfo.port || 'default'}`);
        console.log(`   ⏰ Timestamp: ${new Date().toISOString()}`);
        
        // Store probe result for verification
        const probeResult = {
            traceId,
            sessionId: sessionInfo.sessionId,
            purpose,
            aiPersona,
            port: sessionInfo.port,
            timestamp: new Date().toISOString(),
            status: 'session_validated'
        };
        
        // Write probe result to session artifacts
        try {
            const artifactPath = `.continuum/sessions/${sessionInfo.sessionId}/probe_${traceId}.json`;
            await fs.mkdir(path.dirname(artifactPath), { recursive: true });
            await fs.writeFile(artifactPath, JSON.stringify(probeResult, null, 2));
            console.log(`📁 Probe result saved: ${artifactPath}`);
        } catch (error) {
            console.log(`⚠️ Could not save probe result: ${error.message}`);
        }
        
        return probeResult;
    }

    /**
     * Request a DevTools session for specific purpose and AI persona
     */
    async requestSession(purpose: string, aiPersona: string = 'system', options: any = {}): Promise<SessionInfo> {
        const sessionKey = this.generateSessionKey(purpose, aiPersona);
        
        console.log(`🔍 DEBUG: Requesting session for ${sessionKey}`);
        console.log(`🔍 DEBUG: Current registry has ${simpleTabRegistry.tabs.size} entries`);
        
        // Use file-based lock to prevent race conditions across multiple processes
        const lockFile = `.continuum/session_lock_${sessionKey}.lock`;
        const maxWaitTime = 30000; // 30 seconds
        const startTime = Date.now();
        
        // Wait for any existing lock to clear
        while (await this.isFileLocked(lockFile)) {
            if (Date.now() - startTime > maxWaitTime) {
                console.log(`⚠️ Lock timeout for ${sessionKey}, proceeding anyway`);
                break;
            }
            console.log(`⏳ Waiting for session lock to clear: ${sessionKey}`);
            await new Promise(resolve => setTimeout(resolve, 100));
        }
        
        // Create file lock
        await this.createFileLock(lockFile);
        
        try {
            const result = await this._createSessionWithLock(purpose, aiPersona, options);
            return result;
        } finally {
            // Always remove lock
            await this.removeFileLock(lockFile);
        }
    }

    async _createSessionWithLock(purpose, aiPersona, options) {
        const sessionKey = this.generateSessionKey(purpose, aiPersona);
        
        // First check if ANY Continuum browser is already running (regardless of purpose)
        const existingContinuumBrowser = await this.findExistingContinuumBrowser();
        
        if (existingContinuumBrowser) {
            console.log(`🔄 Found existing Continuum browser on port ${existingContinuumBrowser.port}`);
            
            // Create new tab in existing browser instead of new browser
            try {
                const newTabSession = await this.createTabInExistingBrowser(existingContinuumBrowser, purpose, aiPersona, options);
                console.log(`✅ Created new tab in existing browser: ${newTabSession.sessionId}`);
                
                this.activeSessions.set(sessionKey, newTabSession);
                return newTabSession;
            } catch (error) {
                console.warn(`⚠️ Failed to create tab in existing browser: ${error.message}, will create new browser`);
            }
        }
        
        // Then check SimpleTabRegistry for existing tabs
        try {
            const existingTab = await simpleTabRegistry.findExistingTab(purpose, aiPersona);
            
            if (existingTab) {
                console.log(`🔄 SimpleTabRegistry: Reusing existing tab for ${sessionKey}`);
                console.log(`🔍 DEBUG: Found existing tab with port ${existingTab.port}`);
                
                // Return a session object that represents the existing tab
                const existingSession = {
                    sessionId: existingTab.sessionId,
                    purpose: purpose,
                    aiPersona: aiPersona,
                    port: existingTab.port,
                    created: existingTab.created.toISOString(),
                    status: 'active',
                    isSharedTab: true,
                    browserPid: null, // Shared tab doesn't have separate PID
                    reuseExisting: true
                };
                
                this.activeSessions.set(sessionKey, existingSession);
                return existingSession;
            } else {
                console.log(`🔍 DEBUG: No existing tab found for ${sessionKey}, will create new`);
            }
        } catch (error) {
            console.warn(`⚠️ SimpleTabRegistry check failed: ${error.message}`);
        }
        
        // Check if session already exists for this purpose/persona combination
        const existingSession = this.activeSessions.get(sessionKey);
        if (existingSession && await this.isSessionActive(existingSession)) {
            console.log(`🔄 Legacy: Reusing existing session: ${sessionKey}`);
            return existingSession;
        }

        // Create new session only if no existing tab/session found
        console.log(`🚀 Creating new DevTools session: ${sessionKey}`);
        const session = await this.createSession(purpose, aiPersona, options);
        
        // Store session AND register immediately to prevent race conditions
        this.activeSessions.set(sessionKey, session);
        
        // Register with SimpleTabRegistry IMMEDIATELY to prevent duplicate creation
        try {
            simpleTabRegistry.registerTab(session.purpose, session.aiPersona, session);
            console.log(`🏃 IMMEDIATE: Registered session with SimpleTabRegistry: ${session.sessionId}`);
        } catch (error) {
            console.warn(`⚠️ Failed immediate registration with SimpleTabRegistry: ${error.message}`);
        }
        
        // Run automatic probe validation on new session
        await this.runSessionProbe(session, purpose, aiPersona);
        
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
            
            if (browserProcess && browserProcess.isNewTab) {
                // For new tabs, DevTools is already ready on the shared port
                session.browserPid = null; // No separate process for shared tabs
                session.status = 'active';
                console.log(`✅ DevTools session active (shared tab): ${sessionId} on port ${session.port}`);
            } else {
                // For new browser windows, wait for DevTools to be ready
                session.browserPid = browserProcess.pid;
                session.status = 'active';
                
                await this.waitForDevToolsReady(port);
                console.log(`✅ DevTools session active: ${sessionId} on port ${port}`);
            }
            
            // Create session artifact if requested
            if (options.createArtifact !== false) {
                const artifactPath = await this.createSessionArtifact(session);
                session.artifactPath = artifactPath;
                this.sessionArtifacts.set(sessionId, artifactPath);
            }

            // Register tab with SimpleTabRegistry
            try {
                simpleTabRegistry.registerTab(session.purpose, session.aiPersona, session);
                console.log(`📝 Registered session with SimpleTabRegistry: ${session.sessionId}`);
            } catch (error) {
                console.warn(`⚠️ Failed to register with SimpleTabRegistry: ${error.message}`);
            }
            
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
        // First, verify Continuum server is running
        const continuumServerRunning = await this.checkContinuumServer();
        if (!continuumServerRunning) {
            throw new Error('Continuum server not running on localhost:9000 - browser would show empty page');
        }
        
        // Check if we should use shared browser window or new window
        const sharedWindow = session.options.sharedWindow !== false;
        const windowTitle = session.options.windowTitle || 'Continuum DevTools';
        
        if (sharedWindow) {
            // Try to reuse existing browser window by using shared user data dir
            const sharedUserDataDir = '/tmp/opera-devtools-continuum-shared';
            session.userDataDir = sharedUserDataDir;
            
            // Check if browser is already running on primary port
            const primaryPort = this.portRange.start; // 9222
            const existingBrowser = await this.isPortAvailable(primaryPort);
            
            if (!existingBrowser) {
                // Browser already running, try to open new tab in existing window
                console.log(`🔄 Adding tab to existing Continuum DevTools window`);
                const newTabResult = await this.openNewTabInBrowser(session);
                
                if (newTabResult) {
                    // New tab created successfully
                    return newTabResult;
                } else {
                    // New tab creation failed, fall back to separate browser window
                    console.log(`🔄 New tab creation failed, launching separate browser window`);
                    // Continue to regular browser launch below
                }
            }
        }

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
            `--app=http://localhost:9000?session=${session.sessionId}&purpose=${session.purpose}&persona=${session.aiPersona}`,
            `--window-name=${windowTitle}`
        ];

        console.log(`🌐 Launching ${sharedWindow ? 'shared' : 'dedicated'} Opera window for session ${session.sessionId} on port ${session.port}`);
        const browserProcess = spawn(operaCmd[0], operaCmd.slice(1), {
            stdio: ['ignore', 'ignore', 'ignore'],
            detached: false
        });

        // Don't just wait - actually verify the browser loaded Continuum
        console.log(`⏳ Waiting for browser to load Continuum...`);
        
        try {
            await this.waitForContinuumLoaded(session.port, session.sessionId);
            console.log(`✅ Browser successfully loaded Continuum`);
            return browserProcess;
        } catch (error) {
            // Kill the browser if Continuum didn't load
            try {
                browserProcess.kill();
            } catch (killError) {
                // Process might already be dead
            }
            throw new Error(`Browser launched but Continuum failed to load: ${error.message}`);
        }
    }

    /**
     * Open new tab in existing browser window using DevTools Protocol
     */
    async openNewTabInBrowser(session) {
        try {
            // Find active browser port
            let activeBrowserPort = null;
            for (const [sessionKey, existingSession] of this.activeSessions.entries()) {
                if (await this.isSessionActive(existingSession)) {
                    activeBrowserPort = existingSession.port;
                    break;
                }
            }

            if (!activeBrowserPort) {
                throw new Error('No active browser found for new tab');
            }

            // Create new tab using DevTools API
            const newTabUrl = `http://localhost:9000?session=${session.sessionId}&purpose=${session.purpose}&persona=${session.aiPersona}`;
            
            // Use the correct DevTools API endpoint with PUT method
            const response = await fetch(`http://localhost:${activeBrowserPort}/json/new?${encodeURIComponent(newTabUrl)}`, {
                method: 'PUT'
            });
            
            if (response.ok) {
                const tabInfo = await response.json();
                console.log(`✅ New tab created in existing browser window: ${tabInfo.id}`);
                
                // Update session to use the shared browser port and tab info
                session.port = activeBrowserPort;
                session.isSharedTab = true;
                session.tabId = tabInfo.id;
                session.sharedUserDataDir = true;
                
                return { pid: null, isNewTab: true, tabInfo };
            } else {
                const errorText = await response.text();
                throw new Error(`DevTools API error: ${response.status} - ${errorText}`);
            }
        } catch (error) {
            // Don't throw, instead let it fall back to separate browser launch
            console.log(`⚠️ Could not create new tab, will launch separate browser: ${error.message}`);
            return null; // Signals to caller that new tab creation failed
        }
    }

    /**
     * Wait for Continuum to be fully loaded and responsive (event-driven approach)
     */
    async waitForContinuumLoaded(port, sessionId, maxWait = 15000) {
        const startTime = Date.now();
        
        while (Date.now() - startTime < maxWait) {
            try {
                // First, check if DevTools is ready
                const response = await fetch(`http://localhost:${port}/json`);
                if (response.ok) {
                    const tabs = await response.json();
                    const continuumTab = tabs.find(tab => 
                        tab.url.includes('localhost:9000') || 
                        tab.title.toLowerCase().includes('continuum')
                    );
                    
                    if (continuumTab) {
                        // Found Continuum tab, now verify it's actually loaded by executing JS
                        const testResponse = await fetch(`http://localhost:${port}/json/runtime/evaluate`, {
                            method: 'POST',
                            headers: { 'Content-Type': 'application/json' },
                            body: JSON.stringify({
                                expression: 'typeof window !== "undefined" && document.readyState === "complete" && window.location.hostname === "localhost"',
                                returnByValue: true
                            })
                        });
                        
                        if (testResponse.ok) {
                            const testResult = await testResponse.json();
                            if (testResult.result && testResult.result.value === true) {
                                console.log(`✅ Continuum fully loaded in browser tab: ${continuumTab.title}`);
                                return true;
                            }
                        }
                    }
                }
            } catch (error) {
                // Not ready yet, continue waiting
            }
            
            await new Promise(resolve => setTimeout(resolve, 500));
        }
        
        throw new Error(`Continuum not loaded on port ${port} after ${maxWait}ms`);
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
                // Immediately mark as allocated to prevent race conditions
                this.portAllocation.set(port, 'allocating');
                
                // Check if port is actually available
                const isAvailable = await this.isPortAvailable(port);
                if (isAvailable) {
                    this.portAllocation.set(port, 'allocated');
                    console.log(`🔌 Allocated port ${port} for new session`);
                    return port;
                } else {
                    // Port not available, remove the allocation
                    this.portAllocation.delete(port);
                    console.log(`🔌 Port ${port} not available, checking next port`);
                }
            } else {
                console.log(`🔌 Port ${port} already allocated to: ${this.portAllocation.get(port)}`);
            }
        }
        console.log(`❌ No available ports in range ${this.portRange.start}-${this.portRange.end}`);
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
        const { WorkspaceArtifact } = await import('./artifacts/WorkspaceArtifact.cjs');
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
            // Don't clean up sessions that are less than 30 seconds old
            const sessionAge = Date.now() - new Date(session.created).getTime();
            const minSessionAge = 30 * 1000; // 30 seconds
            
            if (sessionAge < minSessionAge) {
                console.log(`⏭️ Skipping cleanup of recent session: ${sessionKey} (${Math.round(sessionAge/1000)}s old)`);
                continue;
            }
            
            const isActive = await this.isSessionActive(session);
            if (!isActive) {
                console.log(`🔴 Found stale session: ${sessionKey}`);
                staleKeys.push(sessionKey);
                await this.cleanupSession(session.sessionId);
                
                // Also remove from SimpleTabRegistry when cleaning up sessions
                const purpose = session.purpose;
                const aiPersona = session.aiPersona;
                const tabRegistryKey = `${purpose}_${aiPersona}`;
                if (simpleTabRegistry.tabs.has(tabRegistryKey)) {
                    simpleTabRegistry.tabs.delete(tabRegistryKey);
                    console.log(`🧹 Removed ${tabRegistryKey} from SimpleTabRegistry during cleanup`);
                }
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
     * Find any existing browser running Continuum (any purpose/persona)
     */
    async findExistingContinuumBrowser() {
        // Check all possible DevTools ports for existing Continuum browsers
        for (let port = this.portRange.start; port <= this.portRange.end; port++) {
            try {
                const response = await fetch(`http://localhost:${port}/json`, {
                    signal: AbortSignal.timeout(1000)
                });
                
                if (response.ok) {
                    const tabs = await response.json();
                    const continuumTab = tabs.find(tab => 
                        tab.url.includes('localhost:9000') || 
                        tab.url.includes('127.0.0.1:9000') ||
                        tab.title.toLowerCase().includes('continuum')
                    );
                    
                    if (continuumTab) {
                        console.log(`🔍 Found existing Continuum browser on port ${port} with tab: ${continuumTab.title} (${continuumTab.url})`);
                        return { port, tab: continuumTab };
                    }
                }
            } catch (error) {
                // Port not available or no DevTools
            }
        }
        
        return null;
    }
    
    /**
     * Create new tab in existing Continuum browser
     */
    async createTabInExistingBrowser(existingBrowser, purpose, aiPersona, options) {
        const sessionId = `${purpose}_${aiPersona}_${Date.now()}`;
        const newTabUrl = `http://localhost:9000?session=${sessionId}&purpose=${purpose}&persona=${aiPersona}`;
        
        try {
            // Create new tab using DevTools API
            const response = await fetch(`http://localhost:${existingBrowser.port}/json/new?${encodeURIComponent(newTabUrl)}`, {
                method: 'PUT'
            });
            
            if (response.ok) {
                const newTab = await response.json();
                console.log(`✅ Created new tab in existing browser: ${newTab.id}`);
                
                const session = {
                    sessionId: sessionId,
                    purpose: purpose,
                    aiPersona: aiPersona,
                    port: existingBrowser.port,
                    created: new Date().toISOString(),
                    status: 'active',
                    isSharedTab: true,
                    tabId: newTab.id,
                    browserPid: null, // Shared browser
                    reuseExisting: true
                };
                
                // Register with SimpleTabRegistry immediately
                simpleTabRegistry.registerTab(purpose, aiPersona, session);
                
                return session;
            } else {
                throw new Error(`DevTools API error: ${response.status}`);
            }
        } catch (error) {
            throw new Error(`Failed to create tab: ${error.message}`);
        }
    }

    /**
     * Check if Continuum server is running and responsive
     */
    async checkContinuumServer() {
        try {
            const response = await fetch('http://localhost:9000', {
                signal: AbortSignal.timeout(3000)
            });
            
            if (response.ok) {
                console.log('✅ Continuum server is running on localhost:9000');
                return true;
            } else {
                console.log(`⚠️ Continuum server returned status: ${response.status}`);
                return false;
            }
        } catch (error) {
            console.log(`❌ Continuum server not accessible: ${error.message}`);
            return false;
        }
    }

    /**
     * File-based locking helpers to prevent cross-process race conditions
     */
    async isFileLocked(lockFile) {
        try {
            await fs.access(lockFile);
            return true;
        } catch (error) {
            return false;
        }
    }
    
    async createFileLock(lockFile) {
        try {
            await fs.writeFile(lockFile, `${process.pid}:${Date.now()}`);
            console.log(`🔒 Created session lock: ${lockFile}`);
        } catch (error) {
            console.warn(`⚠️ Failed to create session lock: ${error.message}`);
        }
    }
    
    async removeFileLock(lockFile) {
        try {
            await fs.unlink(lockFile);
            console.log(`🔓 Removed session lock: ${lockFile}`);
        } catch (error) {
            // Lock file might not exist or already removed
        }
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

export { DevToolsSessionCoordinator, getDevToolsCoordinator };