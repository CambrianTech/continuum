/**
 * InfoCommand - TypeScript Implementation
 * Parent class for information display commands with full type safety
 */
import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../BaseCommand';
interface InfoParams {
    section?: 'overview' | 'system' | 'server' | 'memory' | 'connections';
}
export declare class InfoCommand extends BaseCommand {
    static getDefinition(): CommandDefinition;
    static execute(params: InfoParams, context?: CommandContext): Promise<CommandResult>;
    /**
     * Parse README.md for command definition
     */
    private static parseReadmeDefinition;
    /**
     * Get typed system information
     */
    private static getSystemInfo;
    /**
     * Get typed server information
     */
    private static getServerInfo;
    /**
     * Get typed memory information
     */
    private static getMemoryInfo;
    /**
     * Get typed connection information
     */
    private static getConnectionInfo;
    /**
     * Get application version
     */
    private static getVersion;
    /**
     * Format system info for display
     */
    private static formatSystemInfo;
    /**
     * Format server info for display
     */
    private static formatServerInfo;
    /**
     * Format memory info for display
     */
    private static formatMemoryInfo;
    /**
     * Format connection info for display
     */
    private static formatConnectionInfo;
    /**
     * Display section with consistent formatting
     */
    private static displaySection;
}
export default InfoCommand;
//# sourceMappingURL=InfoCommand.d.ts.map