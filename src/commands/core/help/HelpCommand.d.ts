/**
 * HelpCommand - TypeScript Implementation
 * Show help information for users and admins with documentation sync
 */
import { InfoCommand } from '../info/InfoCommand';
import { CommandDefinition, CommandContext, CommandResult } from '../BaseCommand';
/**
 * HelpCommand - Show help information and sync documentation
 * Extends InfoCommand for system information display
 */
export declare class HelpCommand extends InfoCommand {
    static getDefinition(): CommandDefinition;
    static execute(params: any, context?: CommandContext): Promise<CommandResult>;
    /**
     * Display main help content
     */
    private static displayHelpContent;
    /**
     * Sync documentation from live help system
     */
    private static syncDocumentation;
    /**
     * Generate markdown documentation from live system
     */
    private static generateMarkdownDocs;
    /**
     * Generate command status table for project management
     */
    private static generateCommandStatusTable;
    /**
     * Load test results from disk
     */
    private static loadTestResults;
    /**
     * Get project health one-liner summary
     */
    private static getProjectHealthOneLiner;
    /**
     * Show verbose dashboard with complete status
     */
    private static showVerboseDashboard;
    /**
     * Get overview content for documentation
     */
    private static getOverviewContent;
    /**
     * Get AI agent content for documentation
     */
    private static getAIAgentContent;
    /**
     * Collect all README files from commands directory
     */
    private static collectAllReadmes;
    /**
     * Get commands list for documentation
     */
    private static getCommandsList;
    /**
     * Get architecture content for documentation
     */
    private static getArchitectureContent;
    /**
     * Get key locations content for documentation
     */
    private static getKeyLocationsContent;
    /**
     * Display header with consistent formatting
     */
    private static displayHeader;
    /**
     * Parse README.md for command definition
     */
    private static parseReadmeDefinition;
}
export default HelpCommand;
//# sourceMappingURL=HelpCommand.d.ts.map