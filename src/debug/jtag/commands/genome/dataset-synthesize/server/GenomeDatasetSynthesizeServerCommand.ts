/**
 * Genome Dataset Synthesize Command - Server Implementation
 *
 * Uses an LLM to synthesize training data (Q&A pairs) for a given topic/skill.
 * The generated data matches the persona's voice and is saved as JSONL
 * compatible with genome/train.
 */

import * as fs from 'fs';
import * as path from 'path';
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
    const systemPrompt = this._buildSystemPrompt(personaName, skill);
    const userPrompt = this._buildUserPrompt(topic, difficulty, exampleCount);

    // Call LLM to generate training data
    const generateParams: Partial<AIGenerateParams> = {
      messages: [
        { role: 'system', content: systemPrompt },
        { role: 'user', content: userPrompt },
      ],
      ...(model && { model }),
      ...(provider && { provider: provider as AIGenerateParams['provider'] }),
      maxTokens: 4096,
      temperature: 0.8,
    };

    const generateResult = await Commands.execute<AIGenerateParams, AIGenerateResult>(
      'ai/generate',
      generateParams,
    );

    if (!generateResult.success || !generateResult.text) {
      return createGenomeDatasetSynthesizeResultFromParams(params, {
        success: false,
        error: generateResult.error ?? 'LLM generation failed — no text returned',
        datasetPath: '',
        exampleCount: 0,
        topic,
        generatedBy: generateResult.model ?? 'unknown',
      });
    }

    // Parse the LLM response into JSONL training examples
    const jsonlLines = this._parseToJSONL(generateResult.text, personaName);

    if (jsonlLines.length === 0) {
      return createGenomeDatasetSynthesizeResultFromParams(params, {
        success: false,
        error: 'LLM produced output but no valid training examples could be parsed',
        datasetPath: '',
        exampleCount: 0,
        topic,
        generatedBy: generateResult.model ?? 'unknown',
      });
    }

    // Save to datasets directory
    const datasetsDir = path.resolve('.continuum/genome/datasets');
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
   * Build the system prompt that sets the persona's voice for data synthesis
   */
  private _buildSystemPrompt(personaName: string, skill: string): string {
    return [
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
      '',
      'Output ONLY a JSON array — no markdown, no explanations, no code fences.',
    ].join('\n');
  }

  /**
   * Build the user prompt requesting specific training examples
   */
  private _buildUserPrompt(topic: string, difficulty: string, count: number): string {
    return [
      `Generate ${count} training conversation examples about: "${topic}"`,
      `Difficulty level: ${difficulty}`,
      '',
      'Each example should have 2-4 message turns (user question, assistant answer, optional follow-up).',
      'Cover diverse aspects of the topic. Make questions natural and varied.',
      '',
      'Output as a JSON array of objects with "messages" arrays.',
    ].join('\n');
  }

  /**
   * Parse LLM output into JSONL training format compatible with genome/train.
   *
   * The LLM should return a JSON array of { messages: [...] } objects.
   * Each gets serialized as one JSONL line.
   */
  private _parseToJSONL(text: string, personaName: string): string[] {
    const lines: string[] = [];

    try {
      // Try to extract JSON array from the text (handle markdown code fences)
      let cleaned = text.trim();
      const jsonMatch = cleaned.match(/\[[\s\S]*\]/);
      if (!jsonMatch) {
        console.warn('   SYNTHESIZE: Could not find JSON array in LLM output');
        return [];
      }

      const examples = JSON.parse(jsonMatch[0]);
      if (!Array.isArray(examples)) {
        console.warn('   SYNTHESIZE: Parsed JSON is not an array');
        return [];
      }

      for (const example of examples) {
        if (!example.messages || !Array.isArray(example.messages)) continue;

        // Validate message structure
        const validMessages = example.messages.every((m: { role?: string; content?: string }) =>
          m.role && m.content && ['system', 'user', 'assistant'].includes(m.role)
        );
        if (!validMessages) continue;

        lines.push(JSON.stringify({ messages: example.messages }));
      }
    } catch (err) {
      console.warn(`   SYNTHESIZE: Failed to parse LLM output as JSON: ${err}`);
    }

    return lines;
  }
}
