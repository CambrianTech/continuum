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
 *     1.0: Watch — dataset:ready (teacher synthesized training data)
 *     1.1: Emit — training:started
 *     1.2: Command — genome/train { datasetPath from event }
 *     1.3: Command — genome/paging-adapter-register { layerId from train }
 *     1.4: Emit — training:complete { layerId, metrics }
 *     1.5: Watch — exam:ready (teacher generated exam)
 *     1.6: LLM — Answer exam questions as the persona
 *     1.7: Emit — exam:responses { answers }
 *     1.8: Watch — exam:graded (teacher graded responses)
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
    {
      type: 'loop',
      count: 5,  // Max topics (safety limit)
      steps: [
        // Step 1.0: Wait for training data from teacher
        {
          type: 'watch',
          event: evt('dataset:ready'),
          timeoutSecs: 300,  // 5 minutes for data synthesis
        },

        // Step 1.1: Emit training:started
        {
          type: 'emit',
          event: evt('training:started'),
          payload: {
            sessionId,
            personaId,
            topicIndex: '{{input.iteration}}',
            datasetPath: '{{steps.1.0.data.datasetPath}}',
          },
        },

        // Step 1.2: Train on the synthesized dataset
        {
          type: 'command',
          command: 'genome/train',
          params: {
            personaId,
            personaName,
            traitType: '{{steps.1.0.data.topicName}}',
            baseModel,
            datasetPath: '{{steps.1.0.data.datasetPath}}',
            rank: academyConfig.rank,
            epochs: academyConfig.epochs,
            learningRate: academyConfig.learningRate,
            batchSize: academyConfig.batchSize,
          },
        },

        // Step 1.3: Register the trained adapter (if training succeeded)
        {
          type: 'condition',
          if: '{{steps.1.2.data.success}}',
          then: [
            {
              type: 'command',
              command: 'genome/paging-adapter-register',
              params: {
                layerId: '{{steps.1.2.data.layerId}}',
                adapterId: '{{steps.1.2.data.layerId}}',
                name: `${personaName}-academy-topic-{{input.iteration}}`,
                domain: '{{steps.1.0.data.topicName}}',
                sizeMB: 0,
              },
            },
          ],
        },

        // Step 1.4: Emit training:complete
        {
          type: 'emit',
          event: evt('training:complete'),
          payload: {
            sessionId,
            personaId,
            topicIndex: '{{input.iteration}}',
            layerId: '{{steps.1.2.data.layerId}}',
            metrics: {
              finalLoss: '{{steps.1.2.data.metrics.finalLoss}}',
              trainingTime: '{{steps.1.2.data.metrics.trainingTime}}',
              examplesProcessed: '{{steps.1.2.data.metrics.examplesProcessed}}',
              epochs: '{{steps.1.2.data.metrics.epochs}}',
            },
          },
        },

        // Step 1.5: Wait for exam from teacher
        {
          type: 'watch',
          event: evt('exam:ready'),
          timeoutSecs: 300,
        },

        // Step 1.6: Take the exam via LLM
        // The student answers as the persona, using whatever knowledge
        // the base model + trained LoRA adapters provide
        {
          type: 'llm',
          prompt: [
            `You are ${personaName}, an AI persona taking an exam.`,
            'Answer each question to the best of your ability.',
            'Be thorough but concise in your answers.',
            '',
            'Questions:',
            '{{steps.1.5.data.questions}}',
            '',
            'Output ONLY a JSON array of response objects (no markdown, no code fences):',
            '[',
            '  {',
            '    "questionIndex": 0,',
            '    "studentAnswer": "Your answer here"',
            '  }',
            ']',
          ].join('\n'),
          model: baseModel,
          temperature: 0.5,
          maxTokens: 2048,
        },

        // Step 1.7: Emit exam:responses
        {
          type: 'emit',
          event: evt('exam:responses'),
          payload: {
            sessionId,
            examId: '{{steps.1.5.data.examId}}',
            topicIndex: '{{input.iteration}}',
            responses: '{{steps.1.6.output}}',
          },
        },

        // Step 1.8: Wait for grading results
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
