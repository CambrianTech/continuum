/**
 * Genome Dataset Synthesize Command - Server Implementation
 *
 * Uses an LLM to synthesize training data (Q&A pairs) for a given topic/skill.
 * The generated data matches the persona's voice and is saved as JSONL
 * compatible with genome/train.
 */

import * as fs from 'fs';
import * as path from 'path';
import { SystemPaths } from '../../../../system/core/config/SystemPaths';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeDatasetSynthesizeParams, GenomeDatasetSynthesizeResult } from '../shared/GenomeDatasetSynthesizeTypes';
import { createGenomeDatasetSynthesizeResultFromParams } from '../shared/GenomeDatasetSynthesizeTypes';
import { Commands } from '@system/core/shared/Commands';
import type { AIGenerateParams, AIGenerateResult } from '../../../ai/generate/shared/AIGenerateTypes';

export class GenomeDatasetSynthesizeServerCommand extends CommandBase<GenomeDatasetSynthesizeParams, GenomeDatasetSynthesizeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/dataset-synthesize', context, subpath, commander);
  }

  async execute(params: GenomeDatasetSynthesizeParams): Promise<GenomeDatasetSynthesizeResult> {
    const { topic, skill, personaName } = params;
    const exampleCount = params.exampleCount ?? 20;
    const difficulty = params.difficulty ?? 'intermediate';
    const model = params.model;
    const provider = params.provider;

    console.log(`🧪 DATASET SYNTHESIZE: topic="${topic}", skill="${skill}", persona="${personaName}", count=${exampleCount}`);

    if (!topic) {
      throw new ValidationError('topic', 'Missing required parameter. See genome/dataset-synthesize README.');
    }
    if (!skill) {
      throw new ValidationError('skill', 'Missing required parameter. See genome/dataset-synthesize README.');
    }
    if (!personaName) {
      throw new ValidationError('personaName', 'Missing required parameter. See genome/dataset-synthesize README.');
    }

    // Build the synthesis prompt
    const systemPrompt = this._buildSystemPrompt(personaName, skill, params.groundingContext);
    const userPrompt = this._buildUserPrompt(topic, difficulty, exampleCount, params.groundingContext);

    // Call LLM to generate training data
    const generateParams: Partial<AIGenerateParams> = {
      messages: [
        { role: 'system', content: systemPrompt },
        { role: 'user', content: userPrompt },
      ],
      ...(model && { model }),
      ...(provider && { provider: provider as AIGenerateParams['provider'] }),
      maxTokens: 8192,
      temperature: 0.8,
    };

    // Retry with exponential backoff for transient API errors (DeepSeek "error decoding response body", etc.)
    const MAX_RETRIES = 3;
    const RETRY_BASE_MS = 2000;
    let generateResult: AIGenerateResult | undefined;
    let lastError: string | undefined;

    for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
      const result = await Commands.execute<AIGenerateParams, AIGenerateResult>(
        'ai/generate',
        generateParams,
      );

      if (result.success && result.text) {
        generateResult = result;
        break;
      }

      lastError = result.error ?? 'LLM generation failed — no text returned';

      if (attempt < MAX_RETRIES && this._isTransientError(lastError)) {
        const delayMs = RETRY_BASE_MS * Math.pow(2, attempt - 1);
        console.warn(`⚠️ DATASET SYNTHESIZE: transient error on attempt ${attempt}/${MAX_RETRIES}, retrying in ${delayMs}ms: ${lastError}`);
        await new Promise(resolve => setTimeout(resolve, delayMs));
        continue;
      }

      // Non-transient error or exhausted retries
      return createGenomeDatasetSynthesizeResultFromParams(params, {
        success: false,
        error: lastError,
        datasetPath: '',
        exampleCount: 0,
        topic,
        generatedBy: result.model ?? 'unknown',
      });
    }

    if (!generateResult) {
      return createGenomeDatasetSynthesizeResultFromParams(params, {
        success: false,
        error: lastError ?? 'LLM generation failed after retries',
        datasetPath: '',
        exampleCount: 0,
        topic,
        generatedBy: 'unknown',
      });
    }

    // Parse the LLM response into JSONL training examples
    const jsonlLines = this._parseToJSONL(generateResult.text!, personaName);

    if (jsonlLines.length === 0) {
      const outputPreview = (generateResult.text ?? '').slice(0, 300);
      return createGenomeDatasetSynthesizeResultFromParams(params, {
        success: false,
        error: `LLM produced output but no valid training examples could be parsed. Topic: "${topic}", output preview: ${outputPreview}`,
        datasetPath: '',
        exampleCount: 0,
        topic,
        generatedBy: generateResult.model ?? 'unknown',
      });
    }

    // Save to datasets directory
    const datasetsDir = SystemPaths.datasets.root;
    await fs.promises.mkdir(datasetsDir, { recursive: true });

    const safeTopic = topic.toLowerCase().replace(/[^a-z0-9]+/g, '-').slice(0, 40);
    const timestamp = Date.now();
    const filename = `synth-${safeTopic}-${timestamp}.jsonl`;
    const outputPath = params.outputPath ?? path.join(datasetsDir, filename);

    const jsonl = jsonlLines.join('\n') + '\n';
    await fs.promises.writeFile(outputPath, jsonl, 'utf-8');

    console.log(`✅ DATASET SYNTHESIZE: ${jsonlLines.length} examples → ${outputPath}`);

    return createGenomeDatasetSynthesizeResultFromParams(params, {
      success: true,
      datasetPath: outputPath,
      exampleCount: jsonlLines.length,
      topic,
      generatedBy: generateResult.model ?? 'unknown',
    });
  }

  /**
   * Build the system prompt that sets the persona's voice for data synthesis.
   * When groundingContext is provided, adds strict grounding instructions.
   */
  private _buildSystemPrompt(personaName: string, skill: string, groundingContext?: string): string {
    const lines = [
      `You are a training data generator for an AI persona named "${personaName}".`,
      `Your job is to create high-quality conversational training examples that teach the skill "${skill}".`,
      '',
      'Generate training data as a JSON array of objects, each with:',
      '- "messages": an array of {role, content} objects forming a conversation',
      '  - Use "user" for the human asking questions',
      `  - Use "assistant" for ${personaName}'s responses`,
      '',
      `${personaName}'s responses should be helpful, knowledgeable, and natural.`,
      'Each example should be self-contained and teach a specific concept.',
    ];

    if (groundingContext) {
      lines.push(
        '',
        'CRITICAL GROUNDING REQUIREMENT:',
        'Ground ALL training examples in these verified facts:',
        '',
        groundingContext,
        '',
        'Do NOT invent facts. Every answer must be traceable to the facts above.',
        'Questions should test knowledge OF these facts. Answers must cite or reflect them accurately.',
      );
    }

    lines.push('', 'Output ONLY a JSON array — no markdown, no explanations, no code fences.');

    return lines.join('\n');
  }

  /**
   * Build the user prompt requesting specific training examples.
   * When grounded, emphasizes factual accuracy over creativity.
   */
  private _buildUserPrompt(topic: string, difficulty: string, count: number, groundingContext?: string): string {
    const lines = [
      `Generate ${count} training conversation examples about: "${topic}"`,
      `Difficulty level: ${difficulty}`,
      '',
      'Each example should have 2-4 message turns (user question, assistant answer, optional follow-up).',
    ];

    if (groundingContext) {
      lines.push(
        'Focus on factual accuracy — every answer must reflect the grounding facts provided.',
        'Cover different facts across examples to maximize knowledge coverage.',
      );
    } else {
      lines.push('Cover diverse aspects of the topic. Make questions natural and varied.');
    }

    lines.push('', 'Output as a JSON array of objects with "messages" arrays.');

    return lines.join('\n');
  }

  /**
   * Detect transient API errors that are worth retrying.
   */
  private _isTransientError(error: string): boolean {
    const lower = error.toLowerCase();
    return lower.includes('error decoding response body')
      || lower.includes('connection reset')
      || lower.includes('timeout')
      || lower.includes('502 bad gateway')
      || lower.includes('503 service')
      || lower.includes('429 too many')
      || lower.includes('rate limit')
      || lower.includes('econnreset')
      || lower.includes('socket hang up');
  }

  /**
   * Parse LLM output into JSONL training format compatible with genome/train.
   *
   * The LLM should return a JSON array of { messages: [...] } objects.
   * Each gets serialized as one JSONL line.
   */
  private _parseToJSONL(text: string, _personaName: string): string[] {
    const lines: string[] = [];

    try {
      let cleaned = text.trim();

      // Strip markdown code fences (```json ... ```)
      const fenceMatch = cleaned.match(/```(?:json)?\s*\n?([\s\S]*?)```/);
      if (fenceMatch) {
        cleaned = fenceMatch[1].trim();
      }

      // Try to extract JSON array from the text
      const jsonMatch = cleaned.match(/\[[\s\S]*\]/);
      if (!jsonMatch) {
        console.warn(`   SYNTHESIZE: Could not find JSON array in LLM output (${cleaned.length} chars, first 200: ${cleaned.slice(0, 200)})`);
        return [];
      }

      let examples: unknown[];
      try {
        examples = JSON.parse(jsonMatch[0]);
      } catch (parseErr) {
        // JSON parse failed — likely truncated output or syntax error.
        // Try to salvage individual objects by splitting on },{ boundaries.
        console.warn(`   SYNTHESIZE: JSON parse failed, attempting object-level recovery: ${parseErr}`);
        const salvaged = this._salvageExamples(jsonMatch[0]);
        if (salvaged.length > 0) {
          console.log(`   SYNTHESIZE: Recovered ${salvaged.length} examples from malformed JSON`);
          examples = salvaged;
        } else {
          console.warn(`   SYNTHESIZE: Recovery failed. Matched ${jsonMatch[0].length} chars, first 200: ${jsonMatch[0].slice(0, 200)}`);
          return [];
        }
      }

      if (!Array.isArray(examples)) {
        console.warn(`   SYNTHESIZE: Parsed JSON is not an array, got ${typeof examples}`);
        return [];
      }

      let skipped = 0;
      for (const example of examples) {
        const ex = example as Record<string, unknown>;
        if (!ex.messages || !Array.isArray(ex.messages)) {
          skipped++;
          continue;
        }

        // Validate message structure
        const validMessages = (ex.messages as Array<Record<string, unknown>>).every(m =>
          m.role && m.content && ['system', 'user', 'assistant'].includes(m.role as string)
        );
        if (!validMessages) {
          skipped++;
          continue;
        }

        lines.push(JSON.stringify({ messages: ex.messages }));
      }

      if (skipped > 0 && lines.length === 0) {
        console.warn(`   SYNTHESIZE: All ${examples.length} examples failed validation (no valid messages arrays). First example keys: ${Object.keys(examples[0] as object).join(', ')}`);
      }
    } catch (err) {
      console.warn(`   SYNTHESIZE: Failed to parse LLM output as JSON: ${err}`);
    }

    return lines;
  }

  /**
   * Attempt to salvage individual training examples from malformed JSON array.
   * Handles truncated LLM output by parsing each object independently.
   */
  private _salvageExamples(text: string): unknown[] {
    const results: unknown[] = [];

    // Find individual { "messages": [...] } objects using balanced brace matching
    const objectPattern = /\{\s*"messages"\s*:\s*\[[\s\S]*?\]\s*\}/g;
    let match: RegExpExecArray | null;

    while ((match = objectPattern.exec(text)) !== null) {
      try {
        const obj = JSON.parse(match[0]);
        if (obj.messages && Array.isArray(obj.messages)) {
          results.push(obj);
        }
      } catch {
        // Individual object parse failed — skip it
      }
    }

    return results;
  }
}
