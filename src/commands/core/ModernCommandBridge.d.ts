/**
 * Modern Command Bridge - Routes TypeScript commands through proper daemon architecture
 * Bridges legacy CommandRegistry with modern CommandProcessorDaemon
 */
import { BaseCommand, CommandContext } from './BaseCommand';
export interface ModernCommandRequest<T = any> {
    command: string;
    params: T;
    context?: CommandContext;
}
export interface ModernCommandResponse<T = any> {
    success: boolean;
    data?: T;
    error?: string;
    message: string;
}
/**
 * Modern Command Bridge - Ensures TypeScript commands get proper context
 * This bridges the gap between legacy CommandRegistry and modern daemon system
 */
export declare class ModernCommandBridge {
    private static continuonStatus;
    private static systemContext;
    /**
     * Initialize bridge with system components
     */
    static initialize(continuum: any): void;
    /**
     * Execute TypeScript command with proper context
     */
    static executeCommand<T, R>(CommandClass: typeof BaseCommand, params: T, continuum?: any): Promise<ModernCommandResponse<R>>;
    /**
     * Get current system context
     */
    static getContext(): CommandContext;
    /**
     * Update system context (for testing or runtime updates)
     */
    static updateContext(updates: Partial<CommandContext>): void;
}
export default ModernCommandBridge;
//# sourceMappingURL=ModernCommandBridge.d.ts.map