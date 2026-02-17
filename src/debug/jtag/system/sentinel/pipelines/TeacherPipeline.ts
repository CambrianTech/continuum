/**
 * TeacherPipeline — Sentinel pipeline template for the Academy teacher
 *
 * The teacher sentinel is the intelligent half of the Academy Dojo.
 * It uses LLM steps to:
 * 1. Research the skill domain and design a progressive curriculum
 * 2. For each topic: synthesize training data, wait for student to train,
 *    generate exams, grade responses, decide pass/fail/remediate
 * 3. Emit events for inter-sentinel coordination with the student
 *
 * All intelligence comes from LLM prompts — the pipeline structure is
 * just control flow. The teacher adapts curriculum based on exam results,
 * generating more data where the student is weak.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { TeacherPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the teacher sentinel pipeline.
 *
 * Step flow:
 *   0: LLM — Research skill, design curriculum (3-5 progressive topics)
 *   1: Command — data/create academy_curricula (persist curriculum)
 *   2: Emit — curriculum:ready
 *   3: Loop (over topics, driven by LLM condition evaluation):
 *     3.0: Command — genome/dataset-synthesize (generate JSONL for current topic)
 *     3.1: Emit — dataset:ready { datasetPath, topicIndex }
 *     3.2: Watch — training:complete (student finished training)
 *     3.3: LLM — Generate exam questions for the current topic
 *     3.4: Command — data/create academy_examinations (persist exam)
 *     3.5: Emit — exam:ready { examId, questions }
 *     3.6: Watch — exam:responses (student submitted answers)
 *     3.7: LLM — Grade responses against expected answers
 *     3.8: Command — data/update academy_examinations (persist grades)
 *     3.9: Emit — exam:graded { scores, passed }
 *     3.10: Condition — if passed, emit topic:passed; else emit topic:remediate
 *   4: Emit — session:complete
 */
export function buildTeacherPipeline(config: TeacherPipelineConfig): Pipeline {
  const { sessionId, skill, personaName, baseModel, config: academyConfig } = config;

  const evt = (action: string) => academyEvent(sessionId, action as any);

  const steps: PipelineStep[] = [
    // Step 0: Design curriculum via LLM
    {
      type: 'llm',
      prompt: [
        `You are designing a training curriculum to teach the skill "${skill}" to an AI persona named "${personaName}".`,
        '',
        `Design a curriculum with 3-5 progressive topics, ordered from foundational to advanced.`,
        `Each topic should build on the previous one.`,
        '',
        'Output ONLY a JSON object with this structure (no markdown, no code fences):',
        '{',
        '  "skill": "the-skill-name",',
        '  "topics": [',
        '    {',
        '      "name": "Topic Name",',
        '      "description": "What this topic covers and why it matters",',
        '      "difficulty": "beginner|intermediate|advanced"',
        '    }',
        '  ]',
        '}',
      ].join('\n'),
      ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
      ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      temperature: 0.7,
      maxTokens: 2048,
    },

    // Step 1: Persist curriculum to database
    {
      type: 'command',
      command: 'data/create',
      params: {
        collection: 'academy_curricula',
        data: {
          sessionId,
          skill,
          topics: '{{steps.0.output}}',  // LLM output parsed by Rust interpolation
          generatedBy: '{{steps.0.data.model}}',
          totalTopics: 0,  // Updated after parsing
          completedTopics: 0,
        },
      },
    },

    // Step 2: Emit curriculum:ready
    {
      type: 'emit',
      event: evt('curriculum:ready'),
      payload: {
        sessionId,
        curriculumId: '{{steps.1.data.id}}',
      },
    },

    // Step 3: Loop over topics
    // Uses 'until' mode — continues until the LLM-evaluated condition is met
    // The loop body handles one topic at a time, advancing via session state
    {
      type: 'loop',
      count: 5,  // Max topics (safety limit matching curriculum max)
      steps: [
        // Step 3.0: Synthesize training data for current topic
        {
          type: 'command',
          command: 'genome/dataset-synthesize',
          params: {
            topic: `{{input.currentTopicName}}`,
            skill,
            personaName,
            exampleCount: academyConfig.examplesPerTopic,
            difficulty: `{{input.currentDifficulty}}`,
            ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
            ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
          },
        },

        // Step 3.1: Emit dataset:ready for student
        {
          type: 'emit',
          event: evt('dataset:ready'),
          payload: {
            sessionId,
            datasetPath: '{{steps.3.0.data.datasetPath}}',
            topicIndex: '{{input.iteration}}',
            topicName: '{{input.currentTopicName}}',
            exampleCount: '{{steps.3.0.data.exampleCount}}',
          },
        },

        // Step 3.2: Wait for student to finish training
        {
          type: 'watch',
          event: evt('training:complete'),
          timeoutSecs: 600,  // 10 minutes for training
        },

        // Step 3.3: Generate exam questions via LLM
        {
          type: 'llm',
          prompt: [
            `Generate ${academyConfig.questionsPerExam} exam questions to test mastery of the topic: "{{input.currentTopicName}}"`,
            `This is part of the "${skill}" curriculum for persona "${personaName}".`,
            `Difficulty: {{input.currentDifficulty}}`,
            '',
            'Output ONLY a JSON array of question objects (no markdown, no code fences):',
            '[',
            '  {',
            '    "question": "The question text",',
            '    "expectedAnswer": "The ideal answer",',
            '    "category": "Sub-category within the topic"',
            '  }',
            ']',
          ].join('\n'),
          ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
          ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
          temperature: 0.7,
          maxTokens: 2048,
        },

        // Step 3.4: Persist exam to database
        {
          type: 'command',
          command: 'data/create',
          params: {
            collection: 'academy_examinations',
            data: {
              sessionId,
              topicIndex: '{{input.iteration}}',
              round: 1,
              questions: '{{steps.3.3.output}}',
              responses: [],
              overallScore: 0,
              passed: false,
            },
          },
        },

        // Step 3.5: Emit exam:ready for student
        {
          type: 'emit',
          event: evt('exam:ready'),
          payload: {
            sessionId,
            examId: '{{steps.3.4.data.id}}',
            topicIndex: '{{input.iteration}}',
            questions: '{{steps.3.3.output}}',
          },
        },

        // Step 3.6: Wait for student responses
        {
          type: 'watch',
          event: evt('exam:responses'),
          timeoutSecs: 300,  // 5 minutes for exam
        },

        // Step 3.7: Grade responses via LLM
        {
          type: 'llm',
          prompt: [
            `Grade the following exam responses for the topic "{{input.currentTopicName}}".`,
            `Passing score: ${academyConfig.passingScore}/100`,
            '',
            'Questions and expected answers:',
            '{{steps.3.3.output}}',
            '',
            'Student responses:',
            '{{steps.3.6.data.responses}}',
            '',
            'For each response, evaluate accuracy and completeness.',
            'Output ONLY a JSON object (no markdown, no code fences):',
            '{',
            '  "overallScore": <0-100>,',
            '  "passed": <true/false>,',
            '  "feedback": "Overall feedback summary",',
            '  "responses": [',
            '    { "questionIndex": 0, "score": <0-100>, "feedback": "Per-question feedback" }',
            '  ]',
            '}',
          ].join('\n'),
          ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
          ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
          temperature: 0.3,  // Lower temperature for consistent grading
          maxTokens: 2048,
        },

        // Step 3.8: Persist grades to database
        {
          type: 'command',
          command: 'data/update',
          params: {
            collection: 'academy_examinations',
            id: '{{steps.3.4.data.id}}',
            data: {
              responses: '{{steps.3.7.output}}',
              overallScore: '{{steps.3.7.data.overallScore}}',
              passed: '{{steps.3.7.data.passed}}',
              gradedBy: '{{steps.3.7.data.model}}',
              feedback: '{{steps.3.7.data.feedback}}',
            },
          },
        },

        // Step 3.9: Emit exam:graded
        {
          type: 'emit',
          event: evt('exam:graded'),
          payload: {
            sessionId,
            examId: '{{steps.3.4.data.id}}',
            topicIndex: '{{input.iteration}}',
            overallScore: '{{steps.3.7.data.overallScore}}',
            passed: '{{steps.3.7.data.passed}}',
            feedback: '{{steps.3.7.data.feedback}}',
          },
        },

        // Step 3.10: Conditional — pass or remediate
        {
          type: 'condition',
          if: '{{steps.3.7.data.passed}}',
          then: [
            {
              type: 'emit',
              event: evt('topic:passed'),
              payload: {
                sessionId,
                topicIndex: '{{input.iteration}}',
              },
            },
          ],
          else: [
            {
              type: 'emit',
              event: evt('topic:remediate'),
              payload: {
                sessionId,
                topicIndex: '{{input.iteration}}',
                feedback: '{{steps.3.7.data.feedback}}',
              },
            },
          ],
        },
      ],
    },

    // Step 4: Emit session:complete
    {
      type: 'emit',
      event: evt('session:complete'),
      payload: {
        sessionId,
        skill,
        personaName,
      },
    },
  ];

  return {
    name: `academy-teacher-${skill}`,
    steps,
    inputs: {
      sessionId,
      skill,
      personaName,
      baseModel,
      currentTopicName: '',  // Set dynamically by curriculum parsing
      currentDifficulty: 'beginner',
    },
  };
}
