/**
 * Browser JavaScript Command - TypeScript Implementation
 * Executes JavaScript code in connected browsers with full type safety
 */
import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../../core/BaseCommand';
interface BrowserJSResult {
    executed: boolean;
    message?: string;
    code?: string;
    encoding?: string;
    timestamp?: string;
    browserResponse?: any;
    output?: any[];
    result?: any;
    error?: string;
    note?: string;
}
/**
 * Browser JavaScript Command - Executes JavaScript in connected browsers
 * Supports base64 encoding for security and auto-conversion for convenience
 */
export declare class BrowserJSCommand extends BaseCommand {
    static getDefinition(): CommandDefinition;
    static execute(params: any, context?: CommandContext): Promise<CommandResult<BrowserJSResult>>;
    /**
     * Process and normalize script parameters from various input formats
     */
    private static processScriptParams;
    /**
     * Execute JavaScript in browser via WebSocket connection
     */
    private static executeBrowserJavaScript;
    /**
     * Validate JavaScript code for basic security and syntax
     */
    private static validateJavaScript;
    /**
     * Generate safe wrapper for JavaScript execution (utility method)
     */
    static generateSafeWrapper(code: string): string;
}
export default BrowserJSCommand;
//# sourceMappingURL=BrowserJSCommand.d.ts.map