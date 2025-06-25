/**
 * Git Hook Bridge - TypeScript → Python Interface
 * ==============================================
 * 
 * Clean TypeScript interface for Python git hook integration.
 * Provides a simple command-line interface that Python can call
 * to use the TypeScript DevTools integration.
 */

import { runGitHookVerification, getGitHookIntegration } from './DevToolsIntegration';
import { DevToolsError } from './types';

/**
 * Command line interface for git hook integration
 */
async function main(): Promise<void> {
    const args = process.argv.slice(2);
    const command = args[0];

    try {
        switch (command) {
            case 'verify':
                await handleVerifyCommand(args.slice(1));
                break;
            
            case 'health':
                await handleHealthCommand();
                break;
            
            case 'session-info':
                await handleSessionInfoCommand();
                break;
            
            case 'cleanup':
                await handleCleanupCommand();
                break;
            
            default:
                printUsage();
                process.exit(1);
        }
    } catch (error) {
        console.error('❌ Git Hook Bridge Error:', error instanceof Error ? error.message : String(error));
        process.exit(1);
    }
}

/**
 * Handle git verification command
 */
async function handleVerifyCommand(args: string[]): Promise<void> {
    if (args.length < 2) {
        console.error('❌ Usage: verify <commitSHA> <commitMessage>');
        process.exit(1);
    }

    const [commitSHA, commitMessage] = args;
    
    console.log(`🔧 TypeScript Git Hook: Starting verification for ${commitSHA.slice(0, 8)}`);
    console.log(`📝 Commit message: ${commitMessage}`);

    const result = await runGitHookVerification(commitSHA, commitMessage);
    
    if (result.success && result.data) {
        console.log('✅ COORDINATED_VERIFICATION_SUCCESS');
        console.log(`🎯 Session coordinated: ${result.data.sessionCoordinated}`);
        console.log(`📸 Screenshot captured: ${result.data.screenshotCaptured}`);
        
        if (result.data.screenshotPath) {
            console.log(`📁 Screenshot path: ${result.data.screenshotPath}`);
        }
        
        // Output verification data for Python to parse
        console.log(`VERIFICATION_DATA:${JSON.stringify(result.data.verificationData)}`);
        
        process.exit(0);
    } else {
        console.error('❌ COORDINATED_VERIFICATION_FAILED');
        if (result.error) {
            console.error(`Error: ${result.error.message}`);
        }
        process.exit(1);
    }
}

/**
 * Handle health check command
 */
async function handleHealthCommand(): Promise<void> {
    const integration = getGitHookIntegration();
    
    try {
        const initResult = await integration.initialize();
        if (!initResult.success) {
            console.error('❌ DevTools API initialization failed');
            process.exit(1);
        }

        const healthResult = await integration.getSessionHealth();
        if (healthResult.success && healthResult.data) {
            console.log('✅ HEALTH_CHECK_SUCCESS');
            console.log(`Sessions: ${healthResult.data.activeSessions}`);
            console.log(`Clients: ${healthResult.data.activeClients}`);
            console.log(`Forwarders: ${healthResult.data.activeForwarders}`);
            console.log(`Tabs: ${healthResult.data.activeTabs}`);
            
            if (healthResult.data.errors.length > 0) {
                console.log('⚠️ Warnings:');
                healthResult.data.errors.forEach((error: string) => {
                    console.log(`  - ${error}`);
                });
            }
        } else {
            console.error('❌ HEALTH_CHECK_FAILED');
            process.exit(1);
        }
    } catch (error) {
        console.error('❌ Health check error:', error instanceof Error ? error.message : String(error));
        process.exit(1);
    }
}

/**
 * Handle session info command
 */
async function handleSessionInfoCommand(): Promise<void> {
    const integration = getGitHookIntegration();
    
    try {
        const currentSession = integration.getCurrentSession();
        const isInProgress = integration.isVerificationInProgress();
        
        console.log('📊 SESSION_INFO');
        console.log(`In progress: ${isInProgress}`);
        
        if (currentSession) {
            console.log(`Session ID: ${currentSession.metadata.sessionId}`);
            console.log(`Port: ${currentSession.metadata.port}`);
            console.log(`Purpose: ${currentSession.config.purpose}`);
            console.log(`AI Persona: ${currentSession.config.aiPersona}`);
            console.log(`Shared tab: ${currentSession.isSharedTab}`);
            console.log(`Created: ${currentSession.metadata.created.toISOString()}`);
        } else {
            console.log('No active session');
        }
    } catch (error) {
        console.error('❌ Session info error:', error instanceof Error ? error.message : String(error));
        process.exit(1);
    }
}

/**
 * Handle cleanup command
 */
async function handleCleanupCommand(): Promise<void> {
    const integration = getGitHookIntegration();
    
    try {
        console.log('🧹 TypeScript Git Hook: Emergency cleanup');
        await integration.emergencyShutdown();
        console.log('✅ CLEANUP_SUCCESS');
    } catch (error) {
        console.error('❌ Cleanup error:', error instanceof Error ? error.message : String(error));
        process.exit(1);
    }
}

/**
 * Print usage information
 */
function printUsage(): void {
    console.log(`
🔧 Git Hook TypeScript Bridge - Usage:

Commands:
  verify <commitSHA> <commitMessage>  - Run git verification with coordinated session
  health                              - Check DevTools API health status
  session-info                        - Get current session information  
  cleanup                             - Emergency cleanup of all sessions

Examples:
  node git-hook-bridge.js verify abc123 "Fix bug in session coordination"
  node git-hook-bridge.js health
  node git-hook-bridge.js session-info
  node git-hook-bridge.js cleanup
`);
}

// Handle uncaught errors gracefully
process.on('unhandledRejection', (reason, promise) => {
    console.error('❌ Unhandled promise rejection:', reason);
    process.exit(1);
});

process.on('uncaughtException', (error) => {
    console.error('❌ Uncaught exception:', error);
    process.exit(1);
});

// Run main function
if (require.main === module) {
    main().catch((error) => {
        console.error('❌ Main function error:', error);
        process.exit(1);
    });
}