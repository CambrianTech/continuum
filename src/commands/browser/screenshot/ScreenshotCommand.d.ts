/**
 * Screenshot Command - TypeScript Implementation
 * Elegant screenshot capture with advanced targeting and orchestration
 */
import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../../core/BaseCommand';
interface ScreenshotResult {
    filename?: string;
    selector: string;
    timestamp: number;
    destination: string;
    requestId: string;
    workflow: string;
    orchestration: boolean;
}
/**
 * Screenshot Command - Captures browser screenshots with advanced targeting
 * Supports README-driven definitions and sophisticated browser orchestration
 */
export declare class ScreenshotCommand extends BaseCommand {
    static getDefinition(): CommandDefinition;
    /**
     * Parse README.md for command definition and parameters
     */
    private static parseReadmeDefinition;
    static execute(params: any, context?: CommandContext): Promise<CommandResult<ScreenshotResult>>;
    /**
     * Setup Continuon animation logging based on animation type
     */
    private static setupContinuonAnimation;
    /**
     * Generate filename based on parameters and destination
     */
    private static generateFilename;
    /**
     * Create the base screenshot message for browser communication
     */
    private static createScreenshotMessage;
    /**
     * Add WSTransfer callback configuration to screenshot message
     */
    private static addWSTransferCallback;
}
export default ScreenshotCommand;
//# sourceMappingURL=ScreenshotCommand.d.ts.map