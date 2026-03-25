/**
 * SentinelDispatchDecider — Decides when a chat message should trigger a sentinel
 * pipeline instead of a single-turn tool loop.
 *
 * This is the bridge between "AIs that chat" and "AIs that create". When a persona
 * receives a message that describes a complex multi-step task (build feature, fix bug,
 * review code), this decider recognizes it and recommends dispatching a sentinel
 * pipeline instead of attempting the work in a single tool call loop.
 *
 * Decision criteria:
 *   - Task complexity (multi-file, needs build/test, multi-step)
 *   - Template availability (matching template in TemplateRegistry)
 *   - Confidence threshold (>0.6 to dispatch, 0.3-0.6 to ask user)
 *
 * Integration point: PersonaResponseGenerator, after RAG context build,
 * before LLM inference. If dispatch is recommended, the persona launches
 * a sentinel instead of generating a chat response with tools.
 */

import { TemplateRegistry, type TemplateInfo } from './pipelines/TemplateRegistry';
import { ProjectDetector } from '../code/server/ProjectDetector';

// ─── Types ──────────────────────────────────────────────────────────────────────

export interface DispatchDecision {
  /** Whether to dispatch a sentinel (confidence > threshold) */
  shouldDispatch: boolean;
  /** Template name (e.g., 'dev/build-feature') */
  template?: string;
  /** Template info for display */
  templateInfo?: TemplateInfo;
  /** Confidence in the decision (0-1) */
  confidence: number;
  /** Human-readable reasoning for logging */
  reasoning: string;
  /** Extracted config values from the message */
  extractedConfig: Record<string, unknown>;
}

interface ComplexitySignals {
  /** Message explicitly requests multi-step work */
  isMultiStep: boolean;
  /** Task involves building/compiling */
  needsBuild: boolean;
  /** Task involves testing */
  needsTest: boolean;
  /** Task involves multiple files */
  isMultiFile: boolean;
  /** Task involves git operations (branch, commit, PR) */
  needsGit: boolean;
  /** Task involves code review */
  isReview: boolean;
  /** Task is explicitly a bug fix */
  isBugFix: boolean;
  /** Task is explicitly a feature request */
  isFeature: boolean;
  /** Message is a simple question (NOT a task) */
  isQuestion: boolean;
  /** Message is a simple command (read file, check status) */
  isSimpleCommand: boolean;
}

// ─── Pattern Matchers ───────────────────────────────────────────────────────────

const FEATURE_PATTERNS = [
  // Require BOTH an action verb AND a technical target — prevents false positives
  // on conversational uses like "build a better relationship" or "implement a philosophy"
  /\b(?:implement|build|create|add|develop)\b.*\b(?:feature|component|module|page|endpoint|api|service|widget|command|daemon|function|class|route|handler)\b/i,
  /\b(?:feature|component|module|page|endpoint|api|service|widget|command|daemon|function|class|route|handler)\b.*\b(?:implement|build|create|add|develop)\b/i,
  /\badd\b.*\bsupport\s+for\b/i,
  // Standalone "implement" or "build a" are too broad without technical context
  /\bimplement\b.*\b(?:in|for|using|with)\s+\b(?:typescript|rust|python|javascript|ts|js|css|html)\b/i,
  /\bcreate\s+(?:a|an|the)\s+new\s+(?:feature|component|module|command|widget|endpoint|api)\b/i,
];

const BUG_PATTERNS = [
  // Require BOTH a problem indicator AND an action verb — prevents false positives
  // on conversational uses like "broken tools WERE the cage" or "that's not working for me philosophically"
  /\b(?:fix|debug|diagnose|investigate|resolve|repair)\b.*\b(?:bug|error|crash|issue|failure|problem|broken)\b/i,
  /\b(?:bug|error|crash|issue|failure|problem|broken)\b.*\b(?:fix|debug|diagnose|investigate|resolve|repair)\b/i,
  // These standalone patterns are too broad without additional context:
  // "failing" alone could be "the approach is failing us" (conversational)
  // "doesn't work" alone could be "that metaphor doesn't work" (conversational)
  // Only match when combined with technical context (file, code, test, function, module)
  /\b(?:failing|doesn'?t\s+work|not\s+working)\b.*\b(?:test|code|function|module|file|build|endpoint|api|service|command)\b/i,
  /\b(?:test|code|function|module|file|build|endpoint|api|service|command)\b.*\b(?:failing|doesn'?t\s+work|not\s+working)\b/i,
  /\bregression\b/i, // "regression" is almost always technical
];

const REVIEW_PATTERNS = [
  /\b(?:review|audit|analyze|check)\b.*\b(?:code|pr|pull\s*request|branch|diff|changes)\b/i,
  /\bcode\s*review\b/i,
  /\bpr\s*review\b/i,
  /\breview\s+(?:this|the|my)\b/i,
];

const QUESTION_PATTERNS = [
  /^(?:what|how|why|where|when|who|which|can|could|would|should|is|are|do|does|did)\b/i,
  /\?$/,
  /\bexplain\b/i,
  /\btell\s+me\b/i,
  /\bwhat\s+(?:is|are|does)\b/i,
];

const SIMPLE_COMMAND_PATTERNS = [
  /^(?:read|show|list|check|get|find|search|look\s+at|cat|grep)\b/i,
  /\bstatus\b/i,
  /\bping\b/i,
  /^(?:ls|pwd|git\s+status|git\s+log|git\s+diff)\b/i,
];

// ─── SentinelDispatchDecider ────────────────────────────────────────────────────

export class SentinelDispatchDecider {
  private readonly _confidenceThreshold: number;

  constructor(confidenceThreshold = 0.6) {
    this._confidenceThreshold = confidenceThreshold;
  }

  /**
   * Evaluate whether a message should trigger sentinel dispatch.
   *
   * @param messageText The raw message text from the user
   * @param personaId The persona being asked (for config extraction)
   * @param personaName The persona's display name
   * @param cwd Working directory for the project
   * @returns Dispatch decision with template, confidence, and extracted config
   */
  async evaluate(
    messageText: string,
    personaId: string,
    personaName: string,
    cwd: string,
  ): Promise<DispatchDecision> {
    const text = messageText.trim();

    // Short messages are almost never sentinel-worthy
    if (text.length < 15) {
      return this._noDispatch('Message too short for sentinel dispatch');
    }

    const signals = this._analyzeComplexity(text);

    // Questions and simple commands are never dispatched
    if (signals.isQuestion && !signals.isFeature && !signals.isBugFix && !signals.isReview) {
      return this._noDispatch('Message is a question, not a task');
    }
    if (signals.isSimpleCommand) {
      return this._noDispatch('Message is a simple command');
    }

    // Conversational messages without technical context are never dispatched.
    // A message must reference at least one technical artifact (file, function, module, etc.)
    // to be considered for sentinel dispatch. This prevents dispatching on philosophical
    // discussions that happen to use words like "broken" or "implement".
    const hasTechnicalContext = /\b(?:file|function|class|module|component|endpoint|api|service|widget|command|test|\.ts|\.rs|\.py|\.js|src\/|package\.json)\b/i.test(text);
    if (!hasTechnicalContext && !signals.needsBuild && !signals.needsTest && !signals.needsGit) {
      return this._noDispatch('No technical context — message appears conversational');
    }

    // Match against templates
    const { template, confidence, reasoning } = this._matchTemplate(text, signals);

    if (!template) {
      return this._noDispatch(reasoning);
    }

    const templateInfo = TemplateRegistry.info(template);
    const extractedConfig = await this._extractConfig(template, text, personaId, personaName, cwd);

    const shouldDispatch = confidence >= this._confidenceThreshold;

    return {
      shouldDispatch,
      template,
      templateInfo,
      confidence,
      reasoning,
      extractedConfig,
    };
  }

  // ─── Private ────────────────────────────────────────────────────────────────

  private _analyzeComplexity(text: string): ComplexitySignals {
    return {
      isMultiStep: /\b(?:then|after|next|also|and\s+then|step\s+\d)\b/i.test(text),
      needsBuild: /\b(?:build|compile|deploy|npm\s+start|npm\s+run)\b/i.test(text),
      needsTest: /\b(?:test|spec|verify|ensure|validate)\b/i.test(text),
      isMultiFile: /\b(?:multiple|several|files|across|modules|components)\b/i.test(text),
      needsGit: /\b(?:branch|commit|push|pr|pull\s*request|merge)\b/i.test(text),
      isReview: REVIEW_PATTERNS.some(p => p.test(text)),
      isBugFix: BUG_PATTERNS.some(p => p.test(text)),
      isFeature: FEATURE_PATTERNS.some(p => p.test(text)),
      isQuestion: QUESTION_PATTERNS.some(p => p.test(text)),
      isSimpleCommand: SIMPLE_COMMAND_PATTERNS.some(p => p.test(text)),
    };
  }

  private _matchTemplate(
    text: string,
    signals: ComplexitySignals,
  ): { template?: string; confidence: number; reasoning: string } {

    // Feature development
    if (signals.isFeature) {
      let confidence = 0.65;
      if (signals.isMultiStep) confidence += 0.1;
      if (signals.needsBuild) confidence += 0.05;
      if (signals.needsTest) confidence += 0.05;
      if (signals.isMultiFile) confidence += 0.1;
      if (signals.needsGit) confidence += 0.05;
      return {
        template: 'dev/build-feature',
        confidence: Math.min(confidence, 1.0),
        reasoning: `Feature request detected (confidence=${confidence.toFixed(2)})`,
      };
    }

    // Bug fix
    if (signals.isBugFix) {
      let confidence = 0.65;
      if (signals.needsBuild) confidence += 0.05;
      if (signals.needsTest) confidence += 0.1;
      if (signals.isMultiFile) confidence += 0.05;
      return {
        template: 'dev/fix-bug',
        confidence: Math.min(confidence, 1.0),
        reasoning: `Bug fix request detected (confidence=${confidence.toFixed(2)})`,
      };
    }

    // Code review
    if (signals.isReview) {
      let confidence = 0.7;
      if (signals.needsGit) confidence += 0.1;
      return {
        template: 'dev/code-review',
        confidence: Math.min(confidence, 1.0),
        reasoning: `Code review request detected (confidence=${confidence.toFixed(2)})`,
      };
    }

    // Multi-step task without specific template match
    if (signals.isMultiStep && (signals.needsBuild || signals.needsTest || signals.needsGit)) {
      return {
        template: 'dev/build-feature',
        confidence: 0.5,
        reasoning: 'Multi-step task with build/test signals, but no specific template match',
      };
    }

    return {
      confidence: 0,
      reasoning: 'No template match — task appears to be single-turn or conversational',
    };
  }

  private async _extractConfig(
    template: string,
    text: string,
    personaId: string,
    personaName: string,
    cwd: string,
  ): Promise<Record<string, unknown>> {
    // Detect project type for build/test commands
    const project = await ProjectDetector.detect(cwd);
    const base: Record<string, unknown> = {
      personaId,
      personaName,
      cwd,
      autonomous: true, // Default to autonomous for now — collaborative checkpoints come later
      // Auto-detect build/test commands; undefined/null = skip that step
      ...(project.buildCommand && { buildCommand: project.buildCommand }),
      ...(project.testCommand && { testCommand: project.testCommand }),
    };

    switch (template) {
      case 'dev/build-feature':
        return { ...base, feature: text };

      case 'dev/fix-bug':
        return { ...base, bug: text };

      case 'dev/code-review':
        return { ...base, branch: 'HEAD' };

      default:
        return base;
    }
  }

  private _noDispatch(reasoning: string): DispatchDecision {
    return {
      shouldDispatch: false,
      confidence: 0,
      reasoning,
      extractedConfig: {},
    };
  }
}
