/**
 * TeacherPipeline — Sentinel pipeline template for the Academy teacher
 *
 * The teacher sentinel is the intelligent half of the Academy Dojo.
 * It uses LLM steps to:
 * 1. Research the skill domain and design a progressive curriculum
 * 2. For each topic: synthesize training data, wait for student to train,
 *    generate exams, grade responses, decide pass/fail/remediate
 * 3. When student fails: generate targeted remedial data, re-train, re-exam
 * 4. Emit events for inter-sentinel coordination with the student
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
 *   3: Outer Loop (over topics, count=5):
 *     outer.0: Command — genome/dataset-synthesize (initial training data)
 *     outer.1: Emit — dataset:ready { datasetPath, topicIndex }
 *     outer.2: Watch — training:complete (student finished initial training)
 *     outer.3: Inner Loop (exam attempts, count=maxTopicAttempts, until=passed):
 *       inner.0: LLM — Generate exam questions
 *       inner.1: Command — data/create academy_examinations
 *       inner.2: Emit — exam:ready { examId, questions }
 *       inner.3: Watch — exam:responses (student answered)
 *       inner.4: LLM — Grade responses
 *       inner.5: Command — data/update academy_examinations
 *       inner.6: Emit — exam:graded { scores, passed }
 *       inner.7: Condition — if passed: emit topic:passed
 *                            else: synthesize remedial data, emit dataset:ready, watch training:complete
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
        curriculumId: '{{steps.1.data.data.id}}',
      },
    },

    // Step 3: Outer loop — iterate over topics
    // {{input.iteration}} is the topic index (0-based)
    // Intra-loop references: outer.N for outer loop steps
    {
      type: 'loop',
      count: 5,  // Max topics (safety limit matching curriculum max)
      steps: [
        // outer.0: Synthesize initial training data for current topic
        {
          type: 'command',
          command: 'genome/dataset-synthesize',
          params: {
            topic: '{{steps.0.output.topics.{{input.iteration}}.name}}',
            skill,
            personaName,
            exampleCount: academyConfig.examplesPerTopic,
            difficulty: '{{steps.0.output.topics.{{input.iteration}}.difficulty}}',
            ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
            ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
          },
        },

        // outer.1: Emit dataset:ready for student (initial training)
        {
          type: 'emit',
          event: evt('dataset:ready'),
          payload: {
            sessionId,
            datasetPath: '{{loop.0.data.datasetPath}}',
            topicIndex: '{{input.iteration}}',
            topicName: '{{steps.0.output.topics.{{input.iteration}}.name}}',
            exampleCount: '{{loop.0.data.exampleCount}}',
          },
        },

        // outer.2: Wait for student to finish initial training
        {
          type: 'watch',
          event: evt('training:complete'),
          timeoutSecs: 600,  // 10 minutes for training
        },

        // outer.3: Inner loop — exam/grade/remediate cycle
        // Uses `until` to break when grading says passed=true.
        // maxIterations is the safety cap (maxTopicAttempts).
        // Each iteration: exam → grade → (if failed) remediate → re-train
        {
          type: 'loop',
          until: '{{loop.4.output.passed}}',
          maxIterations: academyConfig.maxTopicAttempts,
          steps: buildExamRetrySteps(
            sessionId, skill, personaName, academyConfig, evt,
          ),
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
    },
  };
}

/**
 * Build the inner exam retry loop steps.
 *
 * Each iteration of this inner loop:
 * 1. Generates exam questions
 * 2. Emits exam:ready
 * 3. Watches for student responses
 * 4. Grades responses
 * 5. Emits exam:graded
 * 6. If passed: emits topic:passed (loop will terminate via `until`)
 *    If failed: synthesizes targeted remedial data, emits dataset:ready,
 *               waits for re-training to complete
 *
 * Inner loop step references use {{loop.N}} within the inner loop context.
 * The outer loop's topic index is still available via parent context.
 */
function buildExamRetrySteps(
  sessionId: string,
  skill: string,
  personaName: string,
  academyConfig: TeacherPipelineConfig['config'],
  evt: (action: string) => string,
): PipelineStep[] {
  return [
    // inner.0: Generate exam questions via LLM
    {
      type: 'llm',
      prompt: [
        `Generate ${academyConfig.questionsPerExam} exam questions to test mastery of the topic: "{{steps.0.output.topics.{{input.parent_iteration}}.name}}"`,
        `This is part of the "${skill}" curriculum for persona "${personaName}".`,
        `Difficulty: {{steps.0.output.topics.{{input.parent_iteration}}.difficulty}}`,
        `This is exam attempt {{input.iteration}} (0-indexed).`,
        '',
        // On retry attempts, include feedback from previous grading
        '{{#if input.iteration}}',
        'The student failed the previous attempt. Focus questions on weak areas.',
        '{{/if}}',
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

    // inner.1: Persist exam to database
    {
      type: 'command',
      command: 'data/create',
      params: {
        collection: 'academy_examinations',
        data: {
          sessionId,
          topicIndex: '{{input.parent_iteration}}',
          round: '{{input.iteration}}',
          questions: '{{loop.0.output}}',
          responses: [],
          overallScore: 0,
          passed: false,
        },
      },
    },

    // inner.2: Emit exam:ready for student
    {
      type: 'emit',
      event: evt('exam:ready'),
      payload: {
        sessionId,
        examId: '{{loop.1.data.data.id}}',
        topicIndex: '{{input.parent_iteration}}',
        questions: '{{loop.0.output}}',
      },
    },

    // inner.3: Wait for student responses
    {
      type: 'watch',
      event: evt('exam:responses'),
      timeoutSecs: 300,  // 5 minutes for exam
    },

    // inner.4: Grade responses via LLM
    {
      type: 'llm',
      prompt: [
        `Grade the following exam responses for the topic "{{steps.0.output.topics.{{input.parent_iteration}}.name}}".`,
        `Passing score: ${academyConfig.passingScore}/100`,
        `This is attempt {{input.iteration}} (0-indexed).`,
        '',
        'Questions and expected answers:',
        '{{loop.0.output}}',
        '',
        'Student responses:',
        '{{loop.3.data.payload.responses}}',
        '',
        'For each response, evaluate accuracy and completeness.',
        'If the student fails, provide specific feedback on weak areas to guide remediation.',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "overallScore": <0-100>,',
        '  "passed": <true/false>,',
        '  "feedback": "Overall feedback summary with specific weak areas",',
        '  "weakAreas": ["area1", "area2"],',
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

    // inner.5: Persist grades to database
    {
      type: 'command',
      command: 'data/update',
      params: {
        collection: 'academy_examinations',
        id: '{{loop.1.data.data.id}}',
        data: {
          responses: '{{loop.4.output.responses}}',
          overallScore: '{{loop.4.output.overallScore}}',
          passed: '{{loop.4.output.passed}}',
          gradedBy: '{{loop.4.data.model}}',
          feedback: '{{loop.4.output.feedback}}',
          weakAreas: '{{loop.4.output.weakAreas}}',
        },
      },
    },

    // inner.6: Emit exam:graded
    {
      type: 'emit',
      event: evt('exam:graded'),
      payload: {
        sessionId,
        examId: '{{loop.1.data.data.id}}',
        topicIndex: '{{input.parent_iteration}}',
        round: '{{input.iteration}}',
        overallScore: '{{loop.4.output.overallScore}}',
        passed: '{{loop.4.output.passed}}',
        feedback: '{{loop.4.output.feedback}}',
      },
    },

    // inner.7: Pass/remediate decision
    {
      type: 'condition',
      if: '{{loop.4.output.passed}}',
      then: [
        // Student passed — emit topic:passed
        {
          type: 'emit',
          event: evt('topic:passed'),
          payload: {
            sessionId,
            topicIndex: '{{input.parent_iteration}}',
            round: '{{input.iteration}}',
            overallScore: '{{loop.4.output.overallScore}}',
          },
        },
      ],
      else: [
        // Student failed — synthesize targeted remedial data
        {
          type: 'emit',
          event: evt('topic:remediate'),
          payload: {
            sessionId,
            topicIndex: '{{input.parent_iteration}}',
            round: '{{input.iteration}}',
            feedback: '{{loop.4.output.feedback}}',
            weakAreas: '{{loop.4.output.weakAreas}}',
          },
        },

        // Generate remedial training data targeting the weak areas
        {
          type: 'command',
          command: 'genome/dataset-synthesize',
          params: {
            topic: '{{steps.0.output.topics.{{input.parent_iteration}}.name}}',
            skill,
            personaName,
            exampleCount: academyConfig.examplesPerTopic,
            difficulty: '{{steps.0.output.topics.{{input.parent_iteration}}.difficulty}}',
            // Include remediation context for targeted synthesis
            remediationFeedback: '{{loop.4.output.feedback}}',
            weakAreas: '{{loop.4.output.weakAreas}}',
            ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
            ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
          },
        },

        // Emit remedial dataset:ready for student to re-train
        {
          type: 'emit',
          event: evt('dataset:ready'),
          payload: {
            sessionId,
            datasetPath: '{{loop.9.data.datasetPath}}',
            topicIndex: '{{input.parent_iteration}}',
            topicName: '{{steps.0.output.topics.{{input.parent_iteration}}.name}}',
            exampleCount: '{{loop.9.data.exampleCount}}',
            isRemediation: true,
            round: '{{input.iteration}}',
          },
        },

        // Wait for student to finish remedial training
        {
          type: 'watch',
          event: evt('training:complete'),
          timeoutSecs: 600,
        },
      ],
    },
  ];
}
