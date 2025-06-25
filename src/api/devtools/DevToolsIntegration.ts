/**
 * DevTools Integration - TypeScript Implementation
 * ===============================================
 * 
 * Modern TypeScript implementation that properly integrates the DevTools API
 * with git hooks, session coordination, and browser automation.
 * 
 * This file demonstrates the transition to TypeScript with:
 * - Strong typing and interfaces
 * - Proper error handling with custom error types
 * - Clean OOP design with inheritance
 * - Async/await patterns with proper Promise handling
 * - Type-safe event system
 */

import {
    IDevToolsAPI,
    IDevToolsSession,
    SessionPurpose,
    ISessionConfiguration,
    IScreenshotResult,
    IScreenshotOptions,
    ScreenshotFormat,
    SessionState,
    WindowState,
    DevToolsError,
    SessionError,
    IResult,
    EventType
} from './types';

import { TabCoordinator, Tab, getTabCoordinator } from './TabCoordinator';

/**
 * Git Hook Integration Service
 * Manages DevTools sessions specifically for git commit verification
 */
export class GitHookIntegrationService {
    private static instance: GitHookIntegrationService | null = null;
    private devToolsAPI: IDevToolsAPI | null = null;
    private currentSession: IDevToolsSession | null = null;
    private currentTab: Tab | null = null;
    private tabCoordinator: TabCoordinator;
    private verificationInProgress: boolean = false;

    private constructor() {
        // Singleton pattern for git hook integration
        this.tabCoordinator = getTabCoordinator();
    }

    public static getInstance(): GitHookIntegrationService {
        if (!GitHookIntegrationService.instance) {
            GitHookIntegrationService.instance = new GitHookIntegrationService();
        }
        return GitHookIntegrationService.instance;
    }

    /**
     * Initialize DevTools API connection
     */
    public async initialize(): Promise<IResult<boolean>> {
        try {
            // Import the DevTools API (CommonJS module)
            const { getDevToolsAPI } = require('./DevToolsAPI.cjs');
            this.devToolsAPI = getDevToolsAPI();

            // Set up event listeners for session management
            this.setupEventListeners();

            return { success: true, data: true };
        } catch (error) {
            return {
                success: false,
                error: new DevToolsError('Failed to initialize DevTools API', 'INIT_ERROR')
            };
        }
    }

    /**
     * Set up event listeners for DevTools events
     */
    private setupEventListeners(): void {
        if (!this.devToolsAPI) return;

        this.devToolsAPI.on(EventType.SESSION_CREATED, (session: IDevToolsSession) => {
            console.log(`🔧 Git Hook: Session created - ${session.metadata.sessionId}`);
        });

        this.devToolsAPI.on(EventType.SESSION_STOPPED, (sessionId: string) => {
            if (this.currentSession?.metadata.sessionId === sessionId) {
                this.currentSession = null;
                console.log(`🔧 Git Hook: Session stopped - ${sessionId}`);
            }
        });

        this.devToolsAPI.on(EventType.SCREENSHOT_CAPTURED, (result: IScreenshotResult) => {
            console.log(`📸 Git Hook: Screenshot captured - ${result.filename}`);
        });
    }

    /**
     * Request coordinated tab for git verification (prevents duplicates)
     */
    public async requestVerificationTab(): Promise<IResult<Tab>> {
        try {
            // First, check if we already have a verification tab
            const existingTab = this.tabCoordinator.findExistingTab(
                SessionPurpose.GIT_VERIFICATION,
                'system'
            );

            if (existingTab && existingTab.isReady()) {
                console.log(`🔄 Using existing git verification tab: ${existingTab.getId()}`);
                this.currentTab = existingTab;
                return { success: true, data: existingTab };
            }

            // No existing tab, need to create session first
            const sessionResult = await this.requestVerificationSession();
            if (!sessionResult.success) {
                return { success: false, error: sessionResult.error };
            }

            // Get tab information from session
            const tabsResponse = await fetch(`http://localhost:${this.currentSession!.metadata.port}/json`);
            const tabs = await tabsResponse.json();
            
            const continuumTab = tabs.find((tab: any) => tab.url.includes('localhost:9000'));
            if (!continuumTab) {
                return {
                    success: false,
                    error: new SessionError('No Continuum tab found in session', 'git_verification')
                };
            }

            // Register tab with coordinator
            const coordTab = await this.tabCoordinator.registerTab(continuumTab, {
                purpose: SessionPurpose.GIT_VERIFICATION,
                aiPersona: 'system',
                sessionId: this.currentSession!.metadata.sessionId,
                port: this.currentSession!.metadata.port,
                isShared: this.currentSession!.isSharedTab
            });

            this.currentTab = coordTab;
            console.log(`✅ Created new coordinated git verification tab: ${coordTab.getId()}`);
            
            return { success: true, data: coordTab };
        } catch (error) {
            return {
                success: false,
                error: error instanceof Error ? error : new SessionError('Tab coordination failed', 'git_verification')
            };
        }
    }

    /**
     * Request coordinated session for git verification
     */
    public async requestVerificationSession(): Promise<IResult<IDevToolsSession>> {
        if (!this.devToolsAPI) {
            return {
                success: false,
                error: new SessionError('DevTools API not initialized', 'git_verification')
            };
        }

        if (this.verificationInProgress) {
            return {
                success: false,
                error: new SessionError('Verification already in progress', 'git_verification')
            };
        }

        try {
            this.verificationInProgress = true;

            // Create configuration for git verification session
            const sessionConfig: Partial<ISessionConfiguration> = {
                window: {
                    width: 1280,
                    height: 800,
                    x: undefined,
                    y: undefined,
                    state: WindowState.NORMAL,
                    alwaysOnTop: false,
                    resizable: true,
                    title: 'Continuum DevTools - Git Verification',
                    opacity: 1.0
                },
                display: {
                    headless: false,
                    visible: true,
                    kiosk: false,
                    displayId: 'primary',
                    multiMonitor: false
                },
                isolation: {
                    userDataDir: undefined,
                    incognito: false,
                    sharedSession: true,  // Use shared browser window
                    persistData: false,
                    profileName: undefined,
                    cookies: {},
                    localStorage: {}
                },
                automation: {
                    autoClose: false,
                    timeout: 60000, // 1 minute for git verification
                    autoRestart: true,
                    captureErrors: true,
                    captureConsole: true,
                    captureNetwork: false,
                    startUrl: 'http://localhost:9000',
                    preloadScripts: []
                }
            };

            // Request session from DevTools API
            const session = await this.devToolsAPI.requestSession(
                SessionPurpose.GIT_VERIFICATION,
                'system',
                sessionConfig
            );

            this.currentSession = session;

            return { success: true, data: session };
        } catch (error) {
            this.verificationInProgress = false;
            return {
                success: false,
                error: error instanceof Error ? error : new SessionError('Unknown session error', 'git_verification')
            };
        }
    }

    /**
     * Execute verification JavaScript in coordinated tab
     */
    public async executeVerificationScript(commitSHA: string, commitMessage: string): Promise<IResult<any>> {
        if (!this.currentTab) {
            return {
                success: false,
                error: new SessionError('No active verification tab', 'git_verification')
            };
        }

        try {
            // Create verification script with proper TypeScript typing
            const verificationScript = `
                // Git hook verification using coordinated tab
                console.log('🔥 GIT_HOOK_TYPESCRIPT_TAB_VERIFICATION_START');
                
                // Execute verification tests with proper error handling
                const testResults = {
                    tabCoordinated: true,
                    tabId: '${this.currentTab.getId()}',
                    purpose: '${this.currentTab.getPurpose()}',
                    aiPersona: '${this.currentTab.getAIPersona()}',
                    commitSHA: '${commitSHA}',
                    commitMessage: '${commitMessage}',
                    timestamp: new Date().toISOString(),
                    gitHookIntegration: 'WORKING_TYPESCRIPT_TAB_COORDINATOR',
                    verificationLevel: 'PRODUCTION_TAB_MANAGED'
                };
                
                // Test basic UI functionality
                try {
                    const uiElements = document.querySelectorAll('.continuum-ui');
                    testResults.uiElementsFound = uiElements.length;
                    testResults.uiHealthy = uiElements.length > 0;
                } catch (error) {
                    testResults.uiError = error.message;
                    testResults.uiHealthy = false;
                }
                
                // Test console forwarding
                console.log('🎯 TAB_COORDINATED_VERIFICATION:', JSON.stringify(testResults));
                console.log('✅ GIT_HOOK_TAB_VERIFICATION_COMPLETE');
                
                // Return success marker for git hook
                testResults;
            `;

            // Execute script directly in the coordinated tab
            const result = await this.currentTab.executeJavaScript(verificationScript);

            return { success: true, data: result };
        } catch (error) {
            return {
                success: false,
                error: error instanceof Error ? error : new SessionError('Tab script execution failed', this.currentTab.getId())
            };
        }
    }

    /**
     * Capture verification screenshot with TypeScript configuration
     */
    public async captureVerificationScreenshot(filename?: string): Promise<IResult<IScreenshotResult>> {
        if (!this.devToolsAPI || !this.currentSession) {
            return {
                success: false,
                error: new SessionError('No active verification session', 'git_verification')
            };
        }

        try {
            // Configure screenshot options with strong typing
            const screenshotOptions: Partial<IScreenshotOptions> = {
                format: ScreenshotFormat.PNG,
                quality: 90,
                fullPage: false,
                sessionArtifact: true,
                createLatestSymlink: true
            };

            // Generate filename if not provided
            const screenshotFilename = filename || this.generateVerificationFilename();

            // Capture screenshot
            const result = await this.devToolsAPI.takeScreenshot(
                this.currentSession.metadata.sessionId,
                screenshotFilename,
                screenshotOptions
            );

            return { success: true, data: result };
        } catch (error) {
            return {
                success: false,
                error: error instanceof Error ? error : new SessionError('Screenshot capture failed', this.currentSession.metadata.sessionId)
            };
        }
    }

    /**
     * Generate TypeScript-compatible verification filename
     */
    private generateVerificationFilename(): string {
        const timestamp = new Date().toISOString()
            .replace(/[:.-]/g, '')
            .replace('T', '_')
            .slice(0, 15);
        
        return `git_verification_coordinated_${timestamp}.png`;
    }

    /**
     * Complete verification session and cleanup
     */
    public async completeVerification(): Promise<IResult<boolean>> {
        try {
            if (this.currentSession && this.devToolsAPI) {
                // Get final session summary
                const summary = this.devToolsAPI.getSessionSummary();
                console.log(`🎯 Git Hook: Verification complete - ${summary.totalSessions} active sessions`);
                
                // Note: Don't close session immediately, let it be reused for subsequent commits
                // Only mark verification as no longer in progress
                this.verificationInProgress = false;
                
                return { success: true, data: true };
            }

            this.verificationInProgress = false;
            return { success: true, data: true };
        } catch (error) {
            this.verificationInProgress = false;
            return {
                success: false,
                error: error instanceof Error ? error : new DevToolsError('Cleanup failed', 'CLEANUP_ERROR')
            };
        }
    }

    /**
     * Get current session information
     */
    public getCurrentSession(): IDevToolsSession | null {
        return this.currentSession;
    }

    /**
     * Check if verification is in progress
     */
    public isVerificationInProgress(): boolean {
        return this.verificationInProgress;
    }

    /**
     * Get session health information
     */
    public async getSessionHealth(): Promise<IResult<any>> {
        if (!this.devToolsAPI) {
            return {
                success: false,
                error: new DevToolsError('DevTools API not initialized', 'HEALTH_CHECK_ERROR')
            };
        }

        try {
            const health = await this.devToolsAPI.healthCheck();
            return { success: true, data: health };
        } catch (error) {
            return {
                success: false,
                error: error instanceof Error ? error : new DevToolsError('Health check failed', 'HEALTH_CHECK_ERROR')
            };
        }
    }

    /**
     * Emergency shutdown for git hook cleanup
     */
    public async emergencyShutdown(): Promise<void> {
        this.verificationInProgress = false;
        this.currentSession = null;
        
        if (this.devToolsAPI) {
            try {
                await this.devToolsAPI.emergencyShutdown();
            } catch (error) {
                console.error('❌ Emergency shutdown error:', error);
            }
        }
    }
}

/**
 * Factory function to get git hook integration service
 * Provides clean interface for Python git hook to use
 */
export function getGitHookIntegration(): GitHookIntegrationService {
    return GitHookIntegrationService.getInstance();
}

/**
 * Utility function for git hook verification workflow
 * Encapsulates the complete verification process with proper error handling
 */
export async function runGitHookVerification(
    commitSHA: string,
    commitMessage: string
): Promise<IResult<{
    sessionCoordinated: boolean;
    screenshotCaptured: boolean;
    verificationData: any;
    screenshotPath?: string;
}>> {
    const integration = getGitHookIntegration();
    
    try {
        // Initialize DevTools API
        const initResult = await integration.initialize();
        if (!initResult.success) {
            return { success: false, error: initResult.error };
        }

        // Request verification tab (uses TabCoordinator to prevent duplicates)
        const tabResult = await integration.requestVerificationTab();
        if (!tabResult.success) {
            return { success: false, error: tabResult.error };
        }

        // Execute verification script
        const scriptResult = await integration.executeVerificationScript(commitSHA, commitMessage);
        if (!scriptResult.success) {
            return { success: false, error: scriptResult.error };
        }

        // Capture screenshot
        const screenshotResult = await integration.captureVerificationScreenshot();
        const screenshotCaptured = screenshotResult.success;

        // Complete verification
        await integration.completeVerification();

        return {
            success: true,
            data: {
                sessionCoordinated: true,
                screenshotCaptured: screenshotCaptured,
                verificationData: scriptResult.data,
                screenshotPath: screenshotResult.success ? screenshotResult.data?.path : undefined
            }
        };
    } catch (error) {
        // Emergency cleanup on any error
        await integration.emergencyShutdown();
        
        return {
            success: false,
            error: error instanceof Error ? error : new DevToolsError('Unknown verification error', 'VERIFICATION_ERROR')
        };
    }
}

// Export the main integration service and utility functions
export default GitHookIntegrationService;