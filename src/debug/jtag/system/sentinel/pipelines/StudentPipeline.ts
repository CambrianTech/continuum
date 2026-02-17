/**
 * StudentPipeline — Sentinel pipeline template for the Academy student
 *
 * The student sentinel is the learning half of the Academy Dojo.
 * It watches for teacher events and responds:
 * 1. Watches for dataset:ready, trains on the data via genome/train
 * 2. Watches for exam:ready, takes the exam via LLM step
 * 3. Watches for exam:graded to know if it passed or needs remediation
 *
 * The student's intelligence during exams comes from the base model
 * plus any LoRA adapters trained so far — proving that training worked.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { StudentPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the student sentinel pipeline.
 *
 * Step flow:
 *   0: Watch — curriculum:ready (teacher published curriculum)
 *   1: Loop (driven by topic count):
 *     loop.0: Watch — dataset:ready (teacher synthesized training data)
 *     loop.1: Emit — training:started
 *     loop.2: Command — genome/train { datasetPath from event }
 *     loop.3: Condition — register adapter if training succeeded
 *     loop.4: Emit — training:complete { layerId, metrics }
 *     loop.5: Watch — exam:ready (teacher generated exam)
 *     loop.6: LLM — Answer exam questions as the persona
 *     loop.7: Emit — exam:responses { answers }
 *     loop.8: Watch — exam:graded (teacher graded responses)
 */
export function buildStudentPipeline(config: StudentPipelineConfig): Pipeline {
  const { sessionId, personaId, personaName, baseModel, config: academyConfig } = config;

  const evt = (action: string) => academyEvent(sessionId, action as any);

  const steps: PipelineStep[] = [
    // Step 0: Wait for teacher to publish curriculum
    {
      type: 'watch',
      event: evt('curriculum:ready'),
      timeoutSecs: 300,  // 5 minutes for curriculum design
    },

    // Step 1: Loop over topics (matching teacher's topic count)
    // Intra-loop references use {{loop.N.field}} for stable referencing
    {
      type: 'loop',
      count: 5,  // Max topics (safety limit)
      steps: [
        // loop.0: Wait for training data from teacher
        {
          type: 'watch',
          event: evt('dataset:ready'),
          timeoutSecs: 300,  // 5 minutes for data synthesis
        },

        // loop.1: Emit training:started
        {
          type: 'emit',
          event: evt('training:started'),
          payload: {
            sessionId,
            personaId,
            topicIndex: '{{input.iteration}}',
            datasetPath: '{{loop.0.data.payload.datasetPath}}',
          },
        },

        // loop.2: Train on the synthesized dataset
        {
          type: 'command',
          command: 'genome/train',
          params: {
            personaId,
            personaName,
            traitType: '{{loop.0.data.payload.topicName}}',
            baseModel,
            datasetPath: '{{loop.0.data.payload.datasetPath}}',
            rank: academyConfig.rank,
            epochs: academyConfig.epochs,
            learningRate: academyConfig.learningRate,
            batchSize: academyConfig.batchSize,
          },
        },

        // loop.3: Register the trained adapter (if training succeeded)
        {
          type: 'condition',
          if: '{{loop.2.data.success}}',
          then: [
            {
              type: 'command',
              command: 'genome/paging-adapter-register',
              params: {
                layerId: '{{loop.2.data.layerId}}',
                adapterId: '{{loop.2.data.layerId}}',
                name: `${personaName}-${sessionId.slice(0, 8)}-topic-{{input.iteration}}`,
                domain: '{{loop.0.data.payload.topicName}}',
                sizeMB: 0,
              },
            },
          ],
        },

        // loop.4: Emit training:complete
        {
          type: 'emit',
          event: evt('training:complete'),
          payload: {
            sessionId,
            personaId,
            topicIndex: '{{input.iteration}}',
            layerId: '{{loop.2.data.layerId}}',
            metrics: {
              finalLoss: '{{loop.2.data.metrics.finalLoss}}',
              trainingTime: '{{loop.2.data.metrics.trainingTime}}',
              examplesProcessed: '{{loop.2.data.metrics.examplesProcessed}}',
              epochs: '{{loop.2.data.metrics.epochs}}',
            },
          },
        },

        // loop.5: Wait for exam from teacher
        {
          type: 'watch',
          event: evt('exam:ready'),
          timeoutSecs: 300,
        },

        // loop.6: Take the exam via LLM
        // Uses the system default model for now; future: use base model +
        // trained LoRA adapters via Candle local inference to prove training worked
        {
          type: 'llm',
          prompt: [
            `You are ${personaName}, an AI persona taking an exam.`,
            'Answer each question to the best of your ability.',
            'Be thorough but concise in your answers.',
            '',
            'Questions:',
            '{{loop.5.data.payload.questions}}',
            '',
            'Output ONLY a JSON array of response objects (no markdown, no code fences):',
            '[',
            '  {',
            '    "questionIndex": 0,',
            '    "studentAnswer": "Your answer here"',
            '  }',
            ']',
          ].join('\n'),
          temperature: 0.5,
          maxTokens: 2048,
        },

        // loop.7: Emit exam:responses
        {
          type: 'emit',
          event: evt('exam:responses'),
          payload: {
            sessionId,
            examId: '{{loop.5.data.payload.examId}}',
            topicIndex: '{{input.iteration}}',
            responses: '{{loop.6.output}}',
          },
        },

        // loop.8: Wait for grading results
        {
          type: 'watch',
          event: evt('exam:graded'),
          timeoutSecs: 300,
        },
      ],
    },
  ];

  return {
    name: `academy-student-${personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    steps,
    inputs: {
      sessionId,
      personaId,
      personaName,
      baseModel,
    },
  };
}
