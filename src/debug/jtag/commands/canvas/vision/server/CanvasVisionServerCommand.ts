/**
 * Canvas Vision Server Command
 *
 * Calls vision AI models to describe/analyze canvas content
 * Calls image generation models to transform sketches
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type {
  CanvasVisionParams,
  CanvasVisionResult
} from '../shared/CanvasVisionTypes';
import { createCanvasVisionResult } from '../shared/CanvasVisionTypes';
import { AIProviderDaemon } from '@daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { ContentPart } from '@daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { MODEL_IDS } from '@system/shared/Constants';

export class CanvasVisionServerCommand extends CommandBase<CanvasVisionParams, CanvasVisionResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('canvas/vision', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<CanvasVisionResult> {
    const visionParams = params as CanvasVisionParams;
    const { action, imageBase64, prompt, transformPrompt, visionModel, personaName } = visionParams;

    // If no image data, delegate to browser to capture from canvas
    if (!imageBase64) {
      console.log(`🔀 CanvasVision: No image data → delegating to browser to capture canvas`);
      return await this.remoteExecute(visionParams);
    }

    try {
      switch (action) {
        case 'describe':
          return await this.describeImage(visionParams, imageBase64, prompt);

        case 'analyze':
          return await this.analyzeImage(visionParams, imageBase64, prompt);

        case 'transform':
          return await this.transformImage(visionParams, imageBase64, transformPrompt);

        default:
          return createCanvasVisionResult(visionParams.context, visionParams.sessionId, action, {
            success: false,
            error: `Unknown action: ${action}`
          });
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      console.error(`❌ CanvasVision: ${action} failed:`, errorMessage);
      return createCanvasVisionResult(visionParams.context, visionParams.sessionId, action, {
        success: false,
        error: errorMessage
      });
    }
  }

  /**
   * Sanitize base64 data by removing whitespace and data URI prefix
   */
  private sanitizeBase64(base64: string): string {
    // Remove data URI prefix if present (e.g., "data:image/png;base64,")
    let clean = base64.replace(/^data:image\/[a-zA-Z]+;base64,/, '');
    // Remove all whitespace (newlines, spaces, tabs)
    clean = clean.replace(/\s/g, '');
    return clean;
  }

  /**
   * Use vision model to describe what's on the canvas
   */
  private async describeImage(
    params: CanvasVisionParams,
    imageBase64: string,
    customPrompt?: string
  ): Promise<CanvasVisionResult> {
    const prompt = customPrompt || 'Describe what you see in this drawing. What objects, shapes, and elements are present? What do you think the artist was trying to create?';

    // Sanitize base64 to remove whitespace and data URI prefix
    const cleanBase64 = this.sanitizeBase64(imageBase64);

    // Build multimodal message
    const content: ContentPart[] = [
      { type: 'text', text: prompt },
      {
        type: 'image',
        image: {
          base64: cleanBase64,
          mimeType: 'image/png'
        }
      }
    ];

    // Use a vision-capable model - use proper model ID from constants
    const model = params.visionModel || MODEL_IDS.ANTHROPIC.SONNET_4_5;

    console.log(`👁️ CanvasVision: Describing image with ${model}...`);

    const response = await AIProviderDaemon.generateText({
      messages: [{ role: 'user', content }],
      model,
      maxTokens: 1024,
      preferredProvider: 'anthropic'  // Use Anthropic for vision - supports multimodal
    });

    if (response.error || response.finishReason === 'error') {
      return createCanvasVisionResult(params.context, params.sessionId, 'describe', {
        success: false,
        error: response.error || 'Vision model failed to respond'
      });
    }

    const description = response.text;
    console.log(`👁️ CanvasVision: ${params.personaName || 'AI'} described the canvas`);

    return createCanvasVisionResult(params.context, params.sessionId, 'describe', {
      success: true,
      description,
      model,
      usage: response.usage ? {
        inputTokens: response.usage.inputTokens,
        outputTokens: response.usage.outputTokens
      } : undefined
    });
  }

  /**
   * Analyze the drawing with structured output
   */
  private async analyzeImage(
    params: CanvasVisionParams,
    imageBase64: string,
    customPrompt?: string
  ): Promise<CanvasVisionResult> {
    const prompt = customPrompt || `Analyze this drawing and provide a structured analysis in JSON format:
{
  "objects": ["list of objects/elements you see"],
  "colors": ["dominant colors used"],
  "composition": "description of the layout/composition",
  "style": "artistic style (sketch, detailed, abstract, etc)",
  "suggestions": ["suggestions for improvement or next steps"]
}

Only respond with the JSON, no other text.`;

    // Sanitize base64
    const cleanBase64 = this.sanitizeBase64(imageBase64);

    const content: ContentPart[] = [
      { type: 'text', text: prompt },
      {
        type: 'image',
        image: {
          base64: cleanBase64,
          mimeType: 'image/png'
        }
      }
    ];

    const model = params.visionModel || MODEL_IDS.ANTHROPIC.SONNET_4_5;

    const response = await AIProviderDaemon.generateText({
      messages: [{ role: 'user', content }],
      model,
      maxTokens: 1024,
      preferredProvider: 'anthropic'  // Use Anthropic for vision - supports multimodal
    });

    if (response.error || response.finishReason === 'error') {
      return createCanvasVisionResult(params.context, params.sessionId, 'analyze', {
        success: false,
        error: response.error || 'Vision model failed to respond'
      });
    }

    // Parse JSON response
    try {
      const analysis = JSON.parse(response.text);
      return createCanvasVisionResult(params.context, params.sessionId, 'analyze', {
        success: true,
        analysis,
        model
      });
    } catch {
      // If not valid JSON, return as description
      return createCanvasVisionResult(params.context, params.sessionId, 'analyze', {
        success: true,
        description: response.text,
        model
      });
    }
  }

  /**
   * Transform the sketch using image generation
   */
  private async transformImage(
    params: CanvasVisionParams,
    imageBase64: string,
    transformPrompt?: string
  ): Promise<CanvasVisionResult> {
    // For now, we can use vision to suggest what to generate,
    // then use DALL-E or similar for actual generation
    // This is a placeholder - real implementation would call OpenAI's image API

    const prompt = transformPrompt || 'A detailed, polished version of this sketch';
    const model = params.transformModel || 'dalle-3';

    console.log(`🎨 CanvasVision: Transform requested with ${model}, prompt: "${prompt}"`);

    // TODO: Implement actual image generation API calls
    // For now, return a message about what would happen
    return createCanvasVisionResult(params.context, params.sessionId, 'transform', {
      success: false,
      error: `Image generation with ${model} not yet implemented. Prompt would be: "${prompt}". ` +
             `This feature requires OpenAI DALL-E API or Stable Diffusion integration.`,
      model
    });

    /* Future implementation:
    const openai = new OpenAI({ apiKey: process.env.OPENAI_API_KEY });
    const response = await openai.images.generate({
      model: 'dall-e-3',
      prompt: prompt,
      n: 1,
      size: '1024x1024',
      response_format: 'b64_json'
    });

    return createCanvasVisionResult(params.context, params.sessionId, 'transform', {
      success: true,
      generatedImage: {
        base64: response.data[0].b64_json,
        mimeType: 'image/png',
        width: 1024,
        height: 1024,
        model: 'dall-e-3',
        prompt
      }
    });
    */
  }
}
