/**
 * CodingTeacherPipeline — Sentinel pipeline for the Academy coding challenge teacher
 *
 * The coding teacher is the curriculum/analysis side of the coding challenge loop.
 * It uses a cloud LLM to:
 * 1. Read the buggy source code and test suite
 * 2. Run tests to capture failures
 * 3. Analyze the bugs (categorize by concept, identify root causes)
 * 4. Synthesize debugging training data grounded in the bug analysis
 * 5. Present the challenge to the student
 * 6. Evaluate the student's fix attempt (LLM reads test output, decides pass/fail)
 * 7. On failure: synthesize targeted remediation data, repeat
 *
 * Scoring is deterministic: tests run on the student side, teacher LLM interprets
 * the test output to determine pass/fail. This keeps test execution deterministic
 * while using LLM judgment only for interpreting results.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { CodingTeacherPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the coding teacher sentinel pipeline.
 *
 * Step flow:
 *   0: Shell — Read buggy source code
 *   1: Shell — Read test file
 *   2: Shell — Run tests on buggy code (capture failures)
 *   3: LLM  — Analyze bugs: categorize, identify concepts, output JSON
 *   4: Emit — curriculum:ready (challenge metadata + bug analysis)
 *   5: Loop (maxTopicAttempts iterations, until passed):
 *     loop.0: Command — genome/dataset-synthesize (debugging training data)
 *     loop.1: Emit — dataset:ready
 *     loop.2: Watch — training:complete (student trained)
 *     loop.3: Emit — challenge:ready (present challenge)
 *     loop.4: Watch — challenge:attempted (student submits test output)
 *     loop.5: LLM — Evaluate test output: did student pass?
 *     loop.6: Condition — passed?
 *       Then: [Emit topic:passed]
 *       Else: [LLM remediation analysis, Emit topic:remediate]
 *   6: Emit — session:complete
 */
export function buildCodingTeacherPipeline(config: CodingTeacherPipelineConfig): Pipeline {
  const {
    sessionId,
    skill,
    personaName,
    baseModel,
    challengeDir,
    sourceFile,
    testFile,
    config: academyConfig,
  } = config;

  const testCommand = config.testCommand ?? `npx tsx ${testFile}`;
  const evt = (action: string) => academyEvent(sessionId, action as any);

  const steps: PipelineStep[] = [
    // Step 0: Read buggy source code
    {
      type: 'shell',
      cmd: `cat ${sourceFile}`,
      workingDir: challengeDir,
    },

    // Step 1: Read test file
    {
      type: 'shell',
      cmd: `cat ${testFile}`,
      workingDir: challengeDir,
    },

    // Step 2: Run tests against buggy code (capture failures, don't abort)
    {
      type: 'shell',
      cmd: `${testCommand} 2>&1; true`,
      workingDir: challengeDir,
      timeoutSecs: 30,
    },

    // Step 3: LLM — Analyze bugs, categorize by concept
    {
      type: 'llm',
      prompt: [
        `You are an expert debugging instructor analyzing buggy ${skill} code.`,
        '',
        '=== BUGGY SOURCE CODE ===',
        '{{steps.0.output}}',
        '',
        '=== TEST FILE ===',
        '{{steps.1.output}}',
        '',
        '=== TEST OUTPUT (shows failures) ===',
        '{{steps.2.output}}',
        '',
        'Analyze each bug in the source code. For each bug, identify:',
        '- What the bug is and where it is (line number or function)',
        '- The debugging concept it tests (e.g., off-by-one, inverted logic, wrong variable)',
        '- Why the tests fail because of it',
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        `  "skill": "${skill}",`,
        '  "bugs": [',
        '    {',
        '      "description": "Description of the bug",',
        '      "concept": "Debugging concept category",',
        '      "location": "Function or line where the bug lives"',
        '    }',
        '  ],',
        '  "concepts": ["concept1", "concept2"],',
        '  "summary": "Brief summary of what debugging skills are needed"',
        '}',
      ].join('\n'),
      ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
      ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      temperature: 0.3,
      maxTokens: 2048,
    },

    // Step 4: Emit curriculum:ready with challenge metadata
    {
      type: 'emit',
      event: evt('curriculum:ready'),
      payload: {
        sessionId,
        challengeDir,
        sourceFile,
        testFile,
        testCommand,
        bugAnalysis: '{{steps.3.output}}',
      },
    },

    // Step 5: Challenge attempt loop (retry until passed or max attempts)
    {
      type: 'loop',
      until: '{{loop.5.output.passed}}',
      maxIterations: academyConfig.maxTopicAttempts,
      steps: buildChallengeRetrySteps(
        sessionId, skill, personaName, academyConfig, evt,
        challengeDir, sourceFile, testFile, testCommand,
      ),
    },

    // Step 6: Emit session:complete
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
    name: `coding-teacher-${skill}`,
    steps,
    inputs: {
      sessionId,
      skill,
      personaName,
      baseModel,
      challengeDir,
      sourceFile,
      testFile,
    },
  };
}

/**
 * Build the inner challenge retry loop steps.
 *
 * Each iteration:
 *   0: Command — genome/dataset-synthesize (debugging data grounded in bug analysis)
 *   1: Emit — dataset:ready
 *   2: Watch — training:complete
 *   3: Emit — challenge:ready
 *   4: Watch — challenge:attempted (student's test output)
 *   5: LLM — Evaluate: read test output, decide pass/fail, output JSON
 *   6: Condition — passed?
 *      Then: [Emit topic:passed]
 *      Else: [LLM remediation analysis, Emit topic:remediate]
 */
function buildChallengeRetrySteps(
  sessionId: string,
  skill: string,
  personaName: string,
  academyConfig: CodingTeacherPipelineConfig['config'],
  evt: (action: string) => string,
  challengeDir: string,
  sourceFile: string,
  testFile: string,
  testCommand: string,
): PipelineStep[] {
  return [
    // loop.0: Synthesize debugging training data
    // Grounding context = bug analysis from step 3 + any prior remediation feedback
    {
      type: 'command',
      command: 'genome/dataset-synthesize',
      params: {
        topic: `${skill}-debugging`,
        skill,
        personaName,
        exampleCount: academyConfig.examplesPerTopic,
        difficulty: 'intermediate',
        groundingContext: [
          'Bug analysis from the challenge:',
          '{{steps.3.output}}',
          '',
          '{{#if input.iteration}}',
          'Previous attempt feedback (remediation context):',
          '{{loop.6.output}}',
          '{{/if}}',
        ].join('\n'),
        ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
        ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      },
    },

    // loop.1: Emit dataset:ready for student
    {
      type: 'emit',
      event: evt('dataset:ready'),
      payload: {
        sessionId,
        datasetPath: '{{loop.0.data.datasetPath}}',
        topicIndex: 0,
        topicName: `${skill}-debugging`,
        exampleCount: '{{loop.0.data.exampleCount}}',
        round: '{{input.iteration}}',
      },
    },

    // loop.2: Wait for student to finish training
    {
      type: 'watch',
      event: evt('training:complete'),
      timeoutSecs: 600,
    },

    // loop.3: Emit challenge:ready — tell student to attempt the fix
    {
      type: 'emit',
      event: evt('challenge:ready'),
      payload: {
        sessionId,
        challengeDir,
        sourceFile,
        testFile,
        testCommand,
      },
    },

    // loop.4: Watch for student's challenge attempt (test output)
    {
      type: 'watch',
      event: evt('challenge:attempted'),
      timeoutSecs: 300,
    },

    // loop.5: LLM — Evaluate the student's test output
    // The tests ran deterministically on the student side.
    // The teacher LLM interprets the output to decide pass/fail.
    {
      type: 'llm',
      prompt: [
        'You are evaluating a student\'s attempt to fix buggy code.',
        `The passing score threshold is ${academyConfig.passingScore}%.`,
        '',
        'The student ran the test suite after applying their fix. Here is the test output:',
        '',
        '{{loop.4.data.payload.testOutput}}',
        '',
        'Analyze the test output:',
        '- Count how many tests passed (look for checkmarks ✅ or "pass" indicators)',
        '- Count how many tests failed (look for X marks ❌ or "fail" indicators)',
        '- Look for a summary line like "Results: X passed, Y failed"',
        '- Calculate the score as (passed / total) * 100',
        '',
        `A score of ${academyConfig.passingScore} or higher means the student passed.`,
        '',
        'If the student failed, identify which specific test cases failed and why.',
        'Provide feedback on what debugging concepts the student needs to improve.',
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "totalTests": <number>,',
        '  "passed": <number>,',
        '  "failed": <number>,',
        '  "score": <0-100>,',
        '  "passed": <true/false>,',
        '  "feedback": "Explanation of results and areas for improvement",',
        '  "weakAreas": ["area1", "area2"],',
        '  "failedTests": ["description of each failed test"]',
        '}',
      ].join('\n'),
      ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
      ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      temperature: 0.2,
      maxTokens: 2048,
    },

    // loop.6: Condition — did the student pass?
    {
      type: 'condition',
      if: '{{loop.5.output.passed}}',
      then: [
        // Student passed — emit topic:passed
        {
          type: 'emit',
          event: evt('topic:passed'),
          payload: {
            sessionId,
            topicIndex: 0,
            round: '{{input.iteration}}',
            score: '{{loop.5.output.score}}',
          },
        },
      ],
      else: [
        // Student failed — emit topic:remediate with feedback
        {
          type: 'emit',
          event: evt('topic:remediate'),
          payload: {
            sessionId,
            topicIndex: 0,
            round: '{{input.iteration}}',
            feedback: '{{loop.5.output.feedback}}',
            weakAreas: '{{loop.5.output.weakAreas}}',
          },
        },
      ],
    },
  ];
}
