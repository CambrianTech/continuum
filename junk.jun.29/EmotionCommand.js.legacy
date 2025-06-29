/**
 * Emotion Command - TypeScript Implementation
 * Express emotions through continuon animations with full type safety
 */
import { BaseCommand } from '../../core/BaseCommand';
import { VALID_EMOTIONS } from './types';
import { calculateEmotionProperties, shouldReturnHome, isPersistentEmotion } from './emotionConfigs';
import { emotionDefinition } from './emotionDefinition';
export class EmotionCommand extends BaseCommand {
    static getDefinition() {
        return emotionDefinition;
    }
    static async execute(params, context) {
        try {
            // Parse and validate parameters with type safety
            const parsedParams = this.parseParams(params);
            const { feeling = parsedParams.emotion, // Backward compatibility with 'emotion' parameter
            intensity = EmotionCommand.DEFAULT_INTENSITY, duration = EmotionCommand.DEFAULT_DURATION, persist = false, target } = parsedParams;
            // Type-safe emotion validation
            if (!this.isValidEmotion(feeling)) {
                return this.createErrorResult(`Unknown emotion: ${feeling}. Available emotions: ${VALID_EMOTIONS.join(', ')}`);
            }
            // Determine persistence with type safety
            const shouldPersist = persist || isPersistentEmotion(feeling);
            const emotionType = shouldPersist ? 'persistent' : 'fleeting';
            console.log(`💚 Continuon expressing ${feeling} emotion (${emotionType}) with ${intensity} intensity`);
            // Create typed emotion configuration
            const emotionConfig = {
                ...calculateEmotionProperties(feeling, intensity, target),
                target,
                persistent: shouldPersist,
                returnToHome: shouldReturnHome(feeling),
                duration: shouldPersist ? 0 : duration
            };
            // Execute emotion through context
            if (context?.webSocketServer) {
                await this.broadcastEmotion(context.webSocketServer, emotionConfig);
            }
            if (context?.continuonStatus) {
                await this.updateContinuonStatus(context.continuonStatus, feeling, emotionConfig);
            }
            return this.createSuccessResult({
                emotion: feeling,
                config: emotionConfig,
                timestamp: new Date().toISOString()
            });
        }
        catch (error) {
            console.error('❌ Emotion Command Error:', error);
            return this.createErrorResult(`Emotion command failed: ${error.message}`);
        }
    }
    /**
     * Type guard for emotion validation with null safety
     */
    static isValidEmotion(feeling) {
        if (!feeling || typeof feeling !== 'string') {
            return false;
        }
        return VALID_EMOTIONS.includes(feeling.toLowerCase());
    }
    /**
     * Broadcast emotion to all connected clients
     */
    static async broadcastEmotion(webSocketServer, config) {
        const emotionMessage = {
            type: 'emotion_update',
            emotion: config,
            timestamp: new Date().toISOString()
        };
        webSocketServer.broadcast(JSON.stringify(emotionMessage));
        console.log(`📡 Emotion broadcasted: ${config.emoji} (${config.animation})`);
    }
    /**
     * Update continuon visual status
     */
    static async updateContinuonStatus(continuonStatus, emotion, config) {
        if (typeof continuonStatus.setEmotion === 'function') {
            continuonStatus.setEmotion(emotion, config);
            console.log(`🎭 Continuon status updated: ${emotion}`);
        }
    }
    /**
     * Create typed success result
     */
    static createSuccessResult(data) {
        return {
            success: true,
            message: `Emotion '${data.emotion}' expressed successfully`,
            data
        };
    }
    /**
     * Create typed error result
     */
    static createErrorResult(error) {
        return {
            success: false,
            message: 'Emotion command failed',
            error
        };
    }
    /**
     * Type-safe parameter parsing
     */
    static parseParams(params) {
        if (typeof params === 'string') {
            try {
                return JSON.parse(params);
            }
            catch {
                return { feeling: params };
            }
        }
        return params;
    }
}
EmotionCommand.DEFAULT_INTENSITY = 'medium';
EmotionCommand.DEFAULT_DURATION = 3000;
// Default export for easier module loading
export default EmotionCommand;
//# sourceMappingURL=EmotionCommand.js.map