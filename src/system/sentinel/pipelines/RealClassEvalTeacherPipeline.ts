/**
 * RealClassEvalTeacherPipeline — Sentinel pipeline for grading students on
 * real-world Python class implementations from RealClassEval (arxiv:2510.26130).
 *
 * The teacher:
 * 1. Prepares challenges from pre-computed challenges.json (no LLM needed)
 * 2. Loops through ALL challenges (initial exam):
 *    a. Presents skeleton + tests to the student
 *    b. Receives the student's implementation
 *    c. Runs PYNGUIN tests deterministically
 *    d. Parses pytest output to determine pass/fail
 *    e. Writes per-challenge result to tracking file
 * 3. Generates remediation JSONL from reference implementations
 * 4. Emits session:complete with initial scores
 * 5. If training data was generated (failures existed):
 *    a. Waits for student to finish training (reexam:ready)
 *    b. Re-runs the SAME challenges (re-exam)
 *    c. Writes comparison to file
 * 6. Reads comparison file (top-level step for stable interpolation)
 * 7. Emits reexam:complete if comparison data exists
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { RealClassEvalTeacherPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent, type AcademyEventAction } from '../../genome/shared/AcademyTypes';

/**
 * Build the RealClassEval teacher sentinel pipeline.
 *
 * Step flow:
 *   0: Shell — Prepare challenges.json
 *   1: Shell — Read challenges.json
 *   2: Emit — curriculum:ready
 *   3: Loop (initial exam with results tracking)
 *   4: Shell — Generate remediation JSONL
 *   5: Emit — session:complete
 *   6: Condition — failures exist? (datasetPath truthy)
 *     Then: [Watch reexam:ready, Re-exam loop, Shell write comparison]
 *     Else: []
 *   7: Shell — Read comparison.json (allowFailure — no-op if no re-exam)
 *   8: Condition — comparison data exists?
 *     Then: [Emit reexam:complete]
 *     Else: []
 */
export function buildRealClassEvalTeacherPipeline(config: RealClassEvalTeacherPipelineConfig): Pipeline {
  const {
    sessionId,
    skill,
    personaName,
    datasetDir,
    config: academyConfig,
  } = config;

  const evt = (action: string) => academyEvent(sessionId, action as AcademyEventAction);
  const iterEvt = (action: string) => `${academyEvent(sessionId, action as AcademyEventAction)}:{{input.iteration}}`;
  const reexamIterEvt = (action: string) => `${academyEvent(sessionId, `reexam:${action}` as AcademyEventAction)}:{{input.iteration}}`;

  const resultsDir = `${datasetDir}/session-${sessionId.slice(0, 8)}`;

  const steps: PipelineStep[] = [
    // Step 0: Prepare challenges.json from eval.jsonl (deterministic, no LLM)
    {
      type: 'shell',
      cmd: 'python3',
      args: [
        'scripts/prepare-realclasseval-challenges.py',
        datasetDir,
        String(academyConfig.questionsPerExam),
      ],
      timeoutSecs: 30,
    },

    // Step 1: Read the prepared challenges
    {
      type: 'shell',
      cmd: 'cat',
      args: [`${datasetDir}/challenges.json`],
      timeoutSecs: 10,
    },

    // Step 2: Emit curriculum:ready
    {
      type: 'emit',
      event: evt('curriculum:ready'),
      payload: {
        sessionId,
        totalChallenges: '{{steps.1.output.totalChallenges}}',
        skill,
      },
    },

    // Step 3: Initial exam — challenge loop (writes initial-results.jsonl)
    {
      type: 'loop',
      count: academyConfig.questionsPerExam,
      steps: buildChallengeSteps(sessionId, datasetDir, academyConfig, iterEvt, resultsDir, 'initial-results.jsonl'),
    },

    // Step 4: Generate training JSONL from reference implementations
    {
      type: 'shell',
      cmd: 'python3',
      args: [
        'scripts/generate-remediation-jsonl.py',
        datasetDir,
        sessionId,
      ],
      timeoutSecs: 30,
    },

    // Step 5: Emit session:complete with training data path
    {
      type: 'emit',
      event: evt('session:complete'),
      payload: {
        sessionId,
        skill,
        personaName,
        iterationsCompleted: '{{steps.3.data.iterationsCompleted}}',
        datasetPath: '{{steps.4.output.datasetPath}}',
        trainingExamples: '{{steps.4.output.exampleCount}}',
      },
    },

    // Step 6: Condition — any failures? (remediation data exists → re-exam after training)
    {
      type: 'condition',
      if: '{{steps.4.output.datasetPath}}',
      then: [
        // Then.0: Wait for student to finish training
        {
          type: 'watch',
          event: evt('reexam:ready'),
          timeoutSecs: 1800,
        },

        // Then.1: Re-exam loop — same challenges, writes reexam-results.jsonl
        {
          type: 'loop',
          count: academyConfig.questionsPerExam,
          steps: buildChallengeSteps(sessionId, datasetDir, academyConfig, reexamIterEvt, resultsDir, 'reexam-results.jsonl'),
        },

        // Then.2: Compute comparison and write to file (for stable interpolation)
        {
          type: 'shell',
          cmd: [
            'INITIAL_PASSED=$(grep -c \'"passed":true\' "$RESULTS_DIR/initial-results.jsonl" 2>/dev/null || echo 0)',
            'REEXAM_PASSED=$(grep -c \'"passed":true\' "$RESULTS_DIR/reexam-results.jsonl" 2>/dev/null || echo 0)',
            'INITIAL_TOTAL=$(wc -l < "$RESULTS_DIR/initial-results.jsonl" 2>/dev/null | tr -d " " || echo 0)',
            'REEXAM_TOTAL=$(wc -l < "$RESULTS_DIR/reexam-results.jsonl" 2>/dev/null | tr -d " " || echo 0)',
            '[ "$INITIAL_TOTAL" -eq 0 ] && INITIAL_TOTAL=1',
            '[ "$REEXAM_TOTAL" -eq 0 ] && REEXAM_TOTAL=1',
            'INITIAL_PCT=$((INITIAL_PASSED * 100 / INITIAL_TOTAL))',
            'REEXAM_PCT=$((REEXAM_PASSED * 100 / REEXAM_TOTAL))',
            'IMPROVEMENT=$((REEXAM_PASSED - INITIAL_PASSED))',
            'RESULT=$(printf \'{"initialPassed":%d,"reexamPassed":%d,"total":%d,"initialPct":%d,"reexamPct":%d,"improvement":%d}\' "$INITIAL_PASSED" "$REEXAM_PASSED" "$INITIAL_TOTAL" "$INITIAL_PCT" "$REEXAM_PCT" "$IMPROVEMENT")',
            'echo "$RESULT" > "$RESULTS_DIR/comparison.json"',
            'echo "$RESULT"',
          ].join('\n'),
          env: {
            RESULTS_DIR: resultsDir,
          },
          timeoutSecs: 10,
        },
      ],
      else: [],
    },

    // Step 7: Read comparison.json (top-level step = predictable step_index 7)
    // allowFailure: true — file won't exist if no re-exam was run
    {
      type: 'shell',
      cmd: 'cat',
      args: [`${resultsDir}/comparison.json`],
      timeoutSecs: 5,
      allowFailure: true,
    },

    // Step 8: Emit reexam:complete if comparison data exists
    {
      type: 'condition',
      if: '{{steps.7.output.initialPassed}}',
      then: [
        {
          type: 'emit',
          event: evt('reexam:complete'),
          payload: {
            sessionId,
            skill,
            personaName,
            initialPassed: '{{steps.7.output.initialPassed}}',
            reexamPassed: '{{steps.7.output.reexamPassed}}',
            total: '{{steps.7.output.total}}',
            initialPct: '{{steps.7.output.initialPct}}',
            reexamPct: '{{steps.7.output.reexamPct}}',
            improvement: '{{steps.7.output.improvement}}',
          },
        },
      ],
      else: [],
    },
  ];

  return {
    name: `realclasseval-teacher-${skill}`,
    steps,
    inputs: {
      sessionId,
      skill,
      personaName,
      datasetDir,
    },
  };
}

/**
 * Build the inner challenge loop steps.
 *
 * Each iteration processes ONE challenge:
 *   0: Emit challenge:ready (skeleton + tests)
 *   1: Watch challenge:attempted (student submits implementation)
 *   2: Shell — Write student code + run tests
 *   3: Shell — Parse pytest output into structured result (no LLM needed)
 *   4: Condition — passed?
 *      Then: [Emit verdict:ready (passed)]
 *      Else: [Emit verdict:ready (failed, with pytest output)]
 *   5: Shell — Write result to tracking file (for score comparison)
 */
function buildChallengeSteps(
  sessionId: string,
  datasetDir: string,
  academyConfig: RealClassEvalTeacherPipelineConfig['config'],
  iterEvt: (action: string) => string,
  resultsDir: string,
  resultsFile: string,
): PipelineStep[] {
  return [
    // loop.0: Emit challenge:ready (iteration-scoped event name)
    {
      type: 'emit',
      event: iterEvt('challenge:ready'),
      payload: {
        sessionId,
        skeleton: '{{steps.1.output.challenges.{{input.iteration}}.skeleton}}',
        testCode: '{{steps.1.output.challenges.{{input.iteration}}.testCode}}',
        className: '{{steps.1.output.challenges.{{input.iteration}}.className}}',
        moduleName: '{{steps.1.output.challenges.{{input.iteration}}.moduleName}}',
        challengeIndex: '{{input.iteration}}',
      },
    },

    // loop.1: Watch for student's implementation attempt (iteration-scoped)
    {
      type: 'watch',
      event: iterEvt('challenge:attempted'),
      timeoutSecs: 600,
    },

    // loop.2: Write student code to module file + run tests.
    {
      type: 'shell',
      cmd: [
        'TMPDIR=$(mktemp -d)',
        'MODULE_NAME="$CHALLENGE_MODULE"',
        '[ -z "$MODULE_NAME" ] && MODULE_NAME="solution"',
        'printf "%s" "$STUDENT_CODE" > "$TMPDIR/${MODULE_NAME}.py"',
        'printf "%s" "$TEST_CODE" > "$TMPDIR/test_solution.py"',
        'cd "$TMPDIR"',
        'python3 -m pytest test_solution.py -v 2>&1; true',
      ].join('\n'),
      env: {
        STUDENT_CODE: '{{loop.1.data.payload.implementation}}',
        TEST_CODE: '{{steps.1.output.challenges.{{input.iteration}}.testCode}}',
        CHALLENGE_MODULE: '{{steps.1.output.challenges.{{input.iteration}}.moduleName}}',
      },
      timeoutSecs: 60,
    },

    // loop.3: Parse pytest output deterministically (no LLM needed)
    {
      type: 'shell',
      cmd: [
        'PASSED=$(echo "$PYTEST_OUTPUT" | grep -c "PASSED" || true)',
        'FAILED=$(echo "$PYTEST_OUTPUT" | grep -c "FAILED" || true)',
        'XFAILED=$(echo "$PYTEST_OUTPUT" | grep -c "XFAIL" || true)',
        'ERRORS=$(echo "$PYTEST_OUTPUT" | grep -c "ERROR" || true)',
        'TOTAL=$((PASSED + FAILED + XFAILED + ERRORS))',
        '[ "$TOTAL" -eq 0 ] && TOTAL=1',
        'SUCCEEDED=$((PASSED + XFAILED))',
        'SCORE=$(( (SUCCEEDED * 100) / TOTAL ))',
        `PASS_THRESHOLD=${academyConfig.passingScore}`,
        '[ "$SCORE" -ge "$PASS_THRESHOLD" ] && VERDICT="true" || VERDICT="false"',
        'cat << EOF',
        '{"totalTests":$TOTAL,"testsPassed":$SUCCEEDED,"testsFailed":$((FAILED + ERRORS)),"score":$SCORE,"passed":$VERDICT}',
        'EOF',
      ].join('\n'),
      env: {
        PYTEST_OUTPUT: '{{loop.2.output}}',
      },
      timeoutSecs: 10,
    },

    // loop.4: Condition — did the student pass?
    {
      type: 'condition',
      if: '{{loop.3.output.passed}}',
      then: [
        {
          type: 'emit',
          event: iterEvt('verdict:ready'),
          payload: {
            sessionId,
            passed: true,
            challengeIndex: '{{input.iteration}}',
            score: '{{loop.3.output.score}}',
            totalTests: '{{loop.3.output.totalTests}}',
            testsPassed: '{{loop.3.output.testsPassed}}',
          },
        },
      ],
      else: [
        {
          type: 'emit',
          event: iterEvt('verdict:ready'),
          payload: {
            sessionId,
            passed: false,
            challengeIndex: '{{input.iteration}}',
            score: '{{loop.3.output.score}}',
            totalTests: '{{loop.3.output.totalTests}}',
            testsPassed: '{{loop.3.output.testsPassed}}',
            testsFailed: '{{loop.3.output.testsFailed}}',
            pytestOutput: '{{loop.2.output}}',
          },
        },
      ],
    },

    // loop.5: Write result to tracking file (for initial vs re-exam comparison)
    {
      type: 'shell',
      cmd: [
        'mkdir -p "$RESULTS_DIR"',
        'echo "$RESULT_JSON" >> "$RESULTS_DIR/$RESULTS_FILE"',
      ].join('\n'),
      env: {
        RESULTS_DIR: resultsDir,
        RESULTS_FILE: resultsFile,
        RESULT_JSON: '{"challenge":{{input.iteration}},"score":{{loop.3.output.score}},"passed":{{loop.3.output.passed}},"totalTests":{{loop.3.output.totalTests}},"testsPassed":{{loop.3.output.testsPassed}}}',
      },
      timeoutSecs: 5,
    },
  ];
}
