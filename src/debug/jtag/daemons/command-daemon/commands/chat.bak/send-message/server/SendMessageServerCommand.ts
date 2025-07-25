import { SendMessageCommand } from '../shared/SendMessageCommand';
import { SendMessageParams, SendMessageResult } from '../shared/SendMessageTypes';
import type { ICommandDaemon } from '../../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../../shared/JTAGTypes';

export class SendMessageServerCommand extends SendMessageCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: SendMessageParams): Promise<SendMessageResult> {
    try {
      // Validate parameters
      const validation = this.validateParams(params);
      if (!validation.contentValid) {
        return new SendMessageResult({
          roomId: params.roomId,
          success: false,
          deliveryStatus: 'failed',
          error: `Validation failed: ${validation.errors.join(', ')}`,
          contentLength: params.content.length
        });
      }

      // Route message through chat daemon
      const messageId = await this.sendThroughChatDaemon(params);
      
      // Track for Academy if applicable
      if (params.academyContext) {
        await this.trackAcademyInteraction(params, messageId);
      }

      return new SendMessageResult({
        roomId: params.roomId,
        success: true,
        messageId,
        senderId: params.senderId,
        deliveryStatus: 'delivered',
        contentLength: params.content.length,
        messageType: params.messageType,
        deliveryTime: Date.now() - this.startTime,
        academyIntegration: params.academyContext ? {
          sessionId: params.academyContext.sessionId,
          capabilityTracking: true,
          learningValueRecorded: params.learningValue !== undefined
        } : undefined
      });

    } catch (error) {
      return new SendMessageResult({
        roomId: params.roomId,
        success: false,
        deliveryStatus: 'failed',
        error: error instanceof Error ? error.message : 'Unknown error',
        contentLength: params.content.length
      });
    }
  }

  private validateParams(params: SendMessageParams) {
    const errors: string[] = [];
    const warnings: string[] = [];

    // Required fields
    if (!params.roomId) errors.push('roomId is required');
    if (!params.content) errors.push('content is required');

    // Content validation
    if (params.content.length > 4000) {
      errors.push('content exceeds maximum length (4000 characters)');
    }

    // Academy validation
    if (params.academyContext && !params.academyContext.sessionId) {
      errors.push('academyContext requires sessionId');
    }

    // Rate limiting validation (simplified)
    if (this.isRateLimited(params.senderId)) {
      errors.push('rate limit exceeded');
    }

    return {
      contentValid: errors.length === 0,
      lengthValid: params.content.length <= 4000,
      formatValid: true, // Could add markdown validation
      permissionsValid: true, // Could check room permissions
      rateLimitValid: !this.isRateLimited(params.senderId),
      errors,
      warnings
    };
  }

  private async sendThroughChatDaemon(params: SendMessageParams): Promise<string> {
    // This would integrate with the actual chat daemon
    // For now, simulate message sending
    
    const messageId = `msg_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    
    // Route to chat daemon (placeholder)
    // await this.commander.routeToService('chat-daemon', {
    //   operation: 'send-message',
    //   roomId: params.roomId,
    //   content: params.content,
    //   senderId: params.senderId,
    //   messageType: params.messageType
    // });

    // Simulate delivery time
    await new Promise(resolve => setTimeout(resolve, 50));

    return messageId;
  }

  private async trackAcademyInteraction(params: SendMessageParams, messageId: string): Promise<void> {
    if (!params.academyContext) return;

    // Track capability usage if specified
    if (params.capabilitiesUsed && params.capabilitiesUsed.length > 0) {
      // Route to Academy capability tracking
      // await this.commander.routeToCommand('/academy/track-usage', {
      //   sessionId: params.academyContext.sessionId,
      //   capabilities: params.capabilitiesUsed,
      //   messageId,
      //   learningValue: params.learningValue
      // });
    }

    // Record learning value if specified
    if (params.learningValue !== undefined) {
      // Route to Academy learning tracking
      // await this.commander.routeToCommand('/academy/track-learning', {
      //   sessionId: params.academyContext.sessionId,
      //   learningValue: params.learningValue,
      //   messageId,
      //   context: params.academyContext
      // });
    }
  }

  private isRateLimited(senderId?: string): boolean {
    // Simplified rate limiting - would use proper rate limiter in production
    return false;
  }

  private startTime = Date.now();
}