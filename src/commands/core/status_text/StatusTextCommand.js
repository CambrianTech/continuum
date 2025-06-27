/**
 * StatusTextCommand - TypeScript Implementation
 * Update the Continuon status message text with full type safety
 */
import { BaseCommand } from '../BaseCommand';
export class StatusTextCommand extends BaseCommand {
    static getDefinition() {
        return {
            name: 'status_text',
            category: 'core',
            description: 'Update continuon status message text (shows next to ring)',
            icon: '💬',
            params: '{"text": "string", "duration": "number (optional)"}',
            usage: 'Set status message that appears next to continuon ring. Duration in ms for temporary messages.',
            examples: [
                'status_text --params \'{"text": "Processing screenshots..."}\'',
                'status_text --params \'{"text": "Agent taking control", "duration": 5000}\'',
                'status_text --params \'{"text": "Working on task #42"}\'',
                'status_text --params \'{"text": "Ready for commands"}\''
            ]
        };
    }
    static async execute(params, context) {
        try {
            this.logExecution('StatusTextCommand', params, context);
            const { text, duration = 0 } = this.parseParams(params);
            // Validate required parameters
            const validation = this.validateRequired(params, ['text']);
            if (!validation.valid) {
                return this.createErrorResult(`Missing required parameters: ${validation.missing.join(', ')}`);
            }
            // Check if continuonStatus is available
            if (!context?.continuonStatus) {
                return this.createErrorResult('ContinuonStatus not available in context');
            }
            // Update status text through continuon status
            if (typeof context.continuonStatus.updateStatusText === 'function') {
                context.continuonStatus.updateStatusText(text);
                console.log(`💬 Status text updated: "${text}"`);
            }
            else {
                return this.createErrorResult('ContinuonStatus.updateStatusText method not available');
            }
            // Set timeout to revert if duration specified
            if (duration > 0) {
                setTimeout(() => {
                    if (context?.continuonStatus && typeof context.continuonStatus.updateStatusText === 'function') {
                        const defaultText = this.getDefaultStatusText(context.continuonStatus);
                        context.continuonStatus.updateStatusText(defaultText);
                        console.log(`💬 Status text reverted to default after ${duration}ms`);
                    }
                }, duration);
            }
            // Get current status for response
            const currentStatus = this.getCurrentStatus(context.continuonStatus);
            const resultData = {
                text,
                duration,
                temporary: duration > 0,
                status: currentStatus
            };
            return this.createSuccessResult(`Status text updated: "${text}"${duration > 0 ? ` for ${duration}ms` : ' permanently'}`, resultData);
        }
        catch (error) {
            console.error('❌ StatusTextCommand Error:', error);
            return this.createErrorResult(`Status text command failed: ${error.message}`);
        }
    }
    /**
     * Get default status text based on current status
     */
    static getDefaultStatusText(continuonStatus) {
        try {
            if (typeof continuonStatus.currentStatus === 'string') {
                return continuonStatus.currentStatus === 'connected' ? 'Ready' : 'Disconnected';
            }
            return 'Ready';
        }
        catch (error) {
            return 'Ready';
        }
    }
    /**
     * Get current status from continuonStatus
     */
    static getCurrentStatus(continuonStatus) {
        try {
            if (typeof continuonStatus.getStatus === 'function') {
                return continuonStatus.getStatus();
            }
            return { status: 'unknown' };
        }
        catch (error) {
            return { status: 'error', error: error.message };
        }
    }
}
// Default export for easier module loading
export default StatusTextCommand;
//# sourceMappingURL=StatusTextCommand.js.map