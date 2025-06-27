/**
 * Emotion Command - TypeScript Implementation
 * Express emotions through continuon animations with full type safety
 */
import { BaseCommand } from '../../core/BaseCommand';
import { EmotionParams, EmotionContext, EmotionResult } from './types';
export declare class EmotionCommand extends BaseCommand {
    private static readonly DEFAULT_INTENSITY;
    private static readonly DEFAULT_DURATION;
    static getDefinition(): import("../../core/BaseCommand").CommandDefinition;
    static execute(params: EmotionParams, context?: EmotionContext): Promise<EmotionResult>;
    /**
     * Type guard for emotion validation with null safety
     */
    private static isValidEmotion;
    /**
     * Broadcast emotion to all connected clients
     */
    private static broadcastEmotion;
    /**
     * Update continuon visual status
     */
    private static updateContinuonStatus;
    /**
     * Create typed success result
     */
    private static createSuccessResult;
    /**
     * Create typed error result
     */
    private static createErrorResult;
    /**
     * Type-safe parameter parsing
     */
    private static parseParams;
}
export default EmotionCommand;
//# sourceMappingURL=EmotionCommand.d.ts.map