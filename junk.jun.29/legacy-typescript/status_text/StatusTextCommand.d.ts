/**
 * StatusTextCommand - TypeScript Implementation
 * Update the Continuon status message text with full type safety
 */
import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../BaseCommand';
interface StatusTextParams {
    text: string;
    duration?: number;
}
interface StatusTextResult {
    text: string;
    duration: number;
    temporary: boolean;
    status: any;
}
export declare class StatusTextCommand extends BaseCommand {
    static getDefinition(): CommandDefinition;
    static execute(params: StatusTextParams, context?: CommandContext): Promise<CommandResult<StatusTextResult>>;
    /**
     * Get default status text based on current status
     */
    private static getDefaultStatusText;
    /**
     * Get current status from continuonStatus
     */
    private static getCurrentStatus;
}
export default StatusTextCommand;
//# sourceMappingURL=StatusTextCommand.d.ts.map