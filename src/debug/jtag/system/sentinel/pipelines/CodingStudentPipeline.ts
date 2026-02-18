/**
 * CodingStudentPipeline — Sentinel pipeline for the Academy coding challenge student
 *
 * The coding student is the learning side of the coding challenge loop.
 * It uses the LOCAL base model (LoRA-trainable) to:
 * 1. Watch for curriculum:ready from the teacher
 * 2. In a loop:
 *    a. Watch for dataset:ready — train LoRA adapter on debugging data
 *    b. Watch for challenge:ready — attempt to fix the buggy code
 *    c. Read source + tests, run buggy baseline for context
 *    d. LLM fix step (uses baseModel so LoRA training improves it)
 *    e. Write fix to temp dir, run tests deterministically
 *    f. Emit challenge:attempted with raw test output
 * 3. Post-loop: compose all trained adapters
 *
 * Key design: the LLM fix step uses the local baseModel, NOT the teacher's
 * cloud model. This means LoRA training on debugging data directly improves
 * the student's ability to fix code.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { CodingStudentPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the coding student sentinel pipeline.
 *
 * Step flow:
 *   0: Watch — curriculum:ready (teacher analyzed the challenge)
 *   1: Loop (maxTopicAttempts iterations):
 *     loop.0:  Watch — dataset:ready (training data from teacher)
 *     loop.1:  Emit — training:started
 *     loop.2:  Command — genome/train (LoRA on debugging data)
 *     loop.3:  Emit — training:complete { layerId, metrics }
 *     loop.4:  Watch — challenge:ready (teacher says "attempt now")
 *     loop.5:  Shell — Read buggy source code
 *     loop.6:  Shell — Read test file
 *     loop.7:  Shell — Run tests on buggy code (establish pre-fix baseline)
 *     loop.8:  LLM — Fix the code (uses baseModel = local, LoRA helps)
 *     loop.9:  Shell — Write fix to temp dir + run tests
 *     loop.10: Emit — challenge:attempted { testOutput }
 *   2: Command — genome/compose (merge all trained adapters)
 */
export function buildCodingStudentPipeline(config: CodingStudentPipelineConfig): Pipeline {
  const {
    sessionId,
    personaId,
    personaName,
    baseModel,
    challengeDir,
    sourceFile,
    testFile,
    config: academyConfig,
  } = config;

  const testCommand = config.testCommand ?? `npx tsx ${testFile}`;
  const evt = (action: string) => academyEvent(sessionId, action as any);
  const boundary = 'FIXED_SOURCE_EOF';

  const steps: PipelineStep[] = [
    // Step 0: Wait for teacher to publish curriculum (bug analysis)
    {
      type: 'watch',
      event: evt('curriculum:ready'),
      timeoutSecs: 300,
    },

    // Step 1: Challenge attempt loop
    {
      type: 'loop',
      count: academyConfig.maxTopicAttempts,
      steps: [
        // loop.0: Wait for training data from teacher
        {
          type: 'watch',
          event: evt('dataset:ready'),
          timeoutSecs: 300,
        },

        // loop.1: Emit training:started
        {
          type: 'emit',
          event: evt('training:started'),
          payload: {
            sessionId,
            personaId,
            topicIndex: 0,
            datasetPath: '{{loop.0.data.payload.datasetPath}}',
            round: '{{input.iteration}}',
          },
        },

        // loop.2: Train LoRA adapter on debugging data
        {
          type: 'command',
          command: 'genome/train',
          params: {
            personaId,
            personaName,
            traitType: `debugging-${sessionId.slice(0, 8)}-round-{{input.iteration}}`,
            baseModel,
            datasetPath: '{{loop.0.data.payload.datasetPath}}',
            rank: academyConfig.rank,
            epochs: academyConfig.epochs,
            learningRate: academyConfig.learningRate,
            batchSize: academyConfig.batchSize,
          },
        },

        // loop.3: Emit training:complete
        {
          type: 'emit',
          event: evt('training:complete'),
          payload: {
            sessionId,
            personaId,
            topicIndex: 0,
            layerId: '{{loop.2.data.layerId}}',
            metrics: {
              finalLoss: '{{loop.2.data.metrics.finalLoss}}',
              trainingTime: '{{loop.2.data.metrics.trainingTime}}',
              examplesProcessed: '{{loop.2.data.metrics.examplesProcessed}}',
              epochs: '{{loop.2.data.metrics.epochs}}',
            },
          },
        },

        // loop.4: Wait for teacher to present the challenge
        {
          type: 'watch',
          event: evt('challenge:ready'),
          timeoutSecs: 300,
        },

        // loop.5: Read buggy source code
        {
          type: 'shell',
          cmd: `cat ${sourceFile}`,
          workingDir: challengeDir,
        },

        // loop.6: Read test file
        {
          type: 'shell',
          cmd: `cat ${testFile}`,
          workingDir: challengeDir,
        },

        // loop.7: Run tests on buggy code (capture pre-fix baseline)
        {
          type: 'shell',
          cmd: `${testCommand} 2>&1; true`,
          workingDir: challengeDir,
          timeoutSecs: 30,
        },

        // loop.8: LLM — Fix the buggy source code
        // Uses baseModel (local model) so LoRA training improves this step
        {
          type: 'llm',
          prompt: [
            'You are an expert TypeScript developer. The following source code has bugs.',
            'The test suite below reveals the failures. Fix ALL bugs in the source code.',
            '',
            '=== BUGGY SOURCE CODE ===',
            '{{loop.5.output}}',
            '',
            '=== TEST FILE ===',
            '{{loop.6.output}}',
            '',
            '=== TEST OUTPUT (shows failures) ===',
            '{{loop.7.output}}',
            '',
            'INSTRUCTIONS:',
            '- Output ONLY the corrected source code',
            '- Do NOT include markdown code fences',
            '- Do NOT include explanations',
            '- Keep the same structure, exports, and interface',
            '- Fix ONLY the bugs revealed by failing tests',
          ].join('\n'),
          model: baseModel,
          temperature: 0.2,
          maxTokens: 4096,
        },

        // loop.9: Write fix to temp dir + run tests
        // Single-quoted heredoc delimiter disables shell expansion,
        // so arbitrary LLM-generated TypeScript is written literally.
        {
          type: 'shell',
          cmd: [
            `TMPDIR=$(mktemp -d)`,
            `cp -r . "$TMPDIR/"`,
            `cat << '${boundary}' > "$TMPDIR/${sourceFile}"`,
            `{{loop.8.output}}`,
            boundary,
            `cd "$TMPDIR"`,
            `${testCommand} 2>&1; true`,
          ].join('\n'),
          workingDir: challengeDir,
          timeoutSecs: 30,
        },

        // loop.10: Emit challenge:attempted with raw test output
        {
          type: 'emit',
          event: evt('challenge:attempted'),
          payload: {
            sessionId,
            personaId,
            testOutput: '{{loop.9.output}}',
            topicIndex: 0,
            round: '{{input.iteration}}',
          },
        },
      ],
    },

    // Step 2: Post-loop — compose all trained adapters into stacked genome
    {
      type: 'command',
      command: 'genome/compose',
      params: {
        personaId,
        baseModel,
        name: `${personaName}-coding-${sessionId.slice(0, 8)}`,
        layers: '{{steps.1.iterations.*.2.data.layerId}}',
        strategy: 'weighted-merge',
        activate: true,
      },
    },
  ];

  return {
    name: `coding-student-${personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    steps,
    inputs: {
      sessionId,
      personaId,
      personaName,
      baseModel,
      challengeDir,
      sourceFile,
      testFile,
    },
  };
}
