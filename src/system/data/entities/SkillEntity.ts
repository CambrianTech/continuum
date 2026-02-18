/**
 * SkillEntity - Self-modifying skill definition and lifecycle tracking
 *
 * Represents a skill that an AI persona can propose, generate, validate, and activate.
 * Skills are essentially new commands created by the AI team themselves.
 *
 * Lifecycle: proposed → approved → generated → validated → active
 *   (can fail at any stage → 'failed', or be deprecated after activation)
 *
 * Scope:
 *   - 'personal': Only the creator can use it (stored in persona workspace)
 *   - 'team': All personas can use it (requires DecisionProposal approval, stored in commands/)
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  JsonField,
  EnumField,
  CompositeIndex,
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

// ────────────────────────────────────────────────────────────
// Skill status lifecycle
// ────────────────────────────────────────────────────────────

export type SkillStatus =
  | 'proposed'    // AI submitted skill spec, not yet reviewed (team) or ready to generate (personal)
  | 'approved'    // Team approved via DecisionProposal (team-scoped only)
  | 'generated'   // CommandGenerator produced the code files
  | 'validated'   // Compiled + tests passed in sandbox
  | 'active'      // Registered and available for use
  | 'failed'      // Failed at generation, validation, or activation
  | 'deprecated'; // Was active, now retired

export type SkillScope = 'personal' | 'team';

// ────────────────────────────────────────────────────────────
// Skill spec (what gets passed to CommandGenerator)
// ────────────────────────────────────────────────────────────

export interface SkillParamSpec {
  name: string;
  type: string;
  optional?: boolean;
  description?: string;
}

export interface SkillResultSpec {
  name: string;
  type: string;
  description?: string;
}

export interface SkillSpec {
  /** Command name (e.g., 'analysis/complexity') */
  name: string;
  /** What the command does */
  description: string;
  /** Input parameters */
  params: SkillParamSpec[];
  /** Output fields */
  results: SkillResultSpec[];
  /** Usage examples */
  examples?: Array<{
    description: string;
    command: string;
    expectedResult?: string;
  }>;
  /** Natural language description of what the implementation should do */
  implementation: string;
  /** Access level for the command */
  accessLevel?: 'ai-safe' | 'internal' | 'system';
}

// ────────────────────────────────────────────────────────────
// Validation results (populated after skill/validate)
// ────────────────────────────────────────────────────────────

export interface SkillValidationResults {
  compiled: boolean;
  testsRun: number;
  testsPassed: number;
  errors: string[];
  durationMs: number;
}

// ────────────────────────────────────────────────────────────
// Entity
// ────────────────────────────────────────────────────────────

@CompositeIndex({
  name: 'idx_skills_creator_status',
  fields: ['createdById', 'status'],
  direction: 'DESC',
})
@CompositeIndex({
  name: 'idx_skills_scope_status',
  fields: ['scope', 'status'],
  direction: 'DESC',
})
export class SkillEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.SKILLS;

  // ── Identity ──────────────────────────────────────────────

  /** Command name (e.g., 'analysis/complexity', 'code/lint') */
  @TextField({ index: true })
  name!: string;

  /** Human-readable description of what the skill does */
  @TextField()
  description!: string;

  /** AI persona that proposed this skill */
  @TextField({ index: true })
  createdById!: UUID;

  // ── Specification ─────────────────────────────────────────

  /** Full command specification (params, results, examples, implementation) */
  @JsonField()
  spec!: SkillSpec;

  // ── Scope & governance ────────────────────────────────────

  /** Who can use this skill: personal (creator only) or team (all, requires approval) */
  @EnumField({ index: true })
  scope!: SkillScope;

  /** DecisionProposal ID if team-scoped (requires governance approval) */
  @TextField({ nullable: true })
  proposalId?: UUID;

  // ── Lifecycle ─────────────────────────────────────────────

  @EnumField({ index: true })
  status!: SkillStatus;

  /** Error message if status is 'failed' */
  @TextField({ nullable: true })
  failureReason?: string;

  // ── Generation ────────────────────────────────────────────

  /** Directory where generated files live */
  @TextField({ nullable: true })
  outputDir?: string;

  /** Paths of files created by CommandGenerator */
  @JsonField()
  generatedFiles!: string[];

  // ── Validation ────────────────────────────────────────────

  /** Compilation and test results from sandbox validation */
  @JsonField({ nullable: true })
  validationResults?: SkillValidationResults;

  // ── Activation ────────────────────────────────────────────

  /** When the skill was activated (registered as a command) */
  @NumberField({ nullable: true })
  activatedAt?: number;

  // ── Index signature ───────────────────────────────────────

  [key: string]: unknown;

  // ── Constructor ───────────────────────────────────────────

  constructor() {
    super();

    this.name = '';
    this.description = '';
    this.createdById = '' as UUID;
    this.spec = {
      name: '',
      description: '',
      params: [],
      results: [],
      implementation: '',
    };
    this.scope = 'personal';
    this.status = 'proposed';
    this.generatedFiles = [];
  }

  // ── BaseEntity implementation ─────────────────────────────

  get collection(): string {
    return SkillEntity.collection;
  }

  static override getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'createdAt',
      defaultSortDirection: 'desc',
      defaultPageSize: 20,
      cursorField: 'createdAt',
    };
  }

  validate(): { success: boolean; error?: string } {
    if (!this.name?.trim()) {
      return { success: false, error: 'Skill name is required' };
    }

    // Validate command naming convention: category/name or just name
    if (!/^[a-z][a-z0-9-]*(?:\/[a-z][a-z0-9-]*)*$/.test(this.name)) {
      return { success: false, error: `Skill name must follow command naming convention (e.g., 'analysis/complexity'): ${this.name}` };
    }

    if (!this.description?.trim()) {
      return { success: false, error: 'Skill description is required' };
    }

    if (!this.createdById?.trim()) {
      return { success: false, error: 'Skill createdById is required' };
    }

    if (!this.spec || typeof this.spec !== 'object') {
      return { success: false, error: 'Skill spec is required' };
    }

    if (!this.spec.name?.trim()) {
      return { success: false, error: 'Skill spec.name is required' };
    }

    if (this.spec.name !== this.name) {
      return { success: false, error: `Skill spec.name (${this.spec.name}) must match entity name (${this.name})` };
    }

    if (!this.spec.implementation?.trim()) {
      return { success: false, error: 'Skill spec.implementation is required (natural language description)' };
    }

    if (!Array.isArray(this.spec.params)) {
      return { success: false, error: 'Skill spec.params must be an array' };
    }

    if (!Array.isArray(this.spec.results)) {
      return { success: false, error: 'Skill spec.results must be an array' };
    }

    const validScopes: SkillScope[] = ['personal', 'team'];
    if (!validScopes.includes(this.scope)) {
      return { success: false, error: `Skill scope must be one of: ${validScopes.join(', ')}` };
    }

    const validStatuses: SkillStatus[] = [
      'proposed', 'approved', 'generated', 'validated', 'active', 'failed', 'deprecated',
    ];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `Skill status must be one of: ${validStatuses.join(', ')}` };
    }

    return { success: true };
  }

  // ── Convenience properties ────────────────────────────────

  /** Whether this skill has been activated and is available for use */
  get isActive(): boolean {
    return this.status === 'active';
  }

  /** Whether this skill requires team approval */
  get requiresApproval(): boolean {
    return this.scope === 'team';
  }

  /** Whether this skill can proceed to the next lifecycle stage */
  get canAdvance(): boolean {
    switch (this.status) {
      case 'proposed': return this.scope === 'personal' || !!this.proposalId;
      case 'approved': return true; // Can generate
      case 'generated': return true; // Can validate
      case 'validated': return true; // Can activate
      default: return false;
    }
  }

  /** The next expected status in the lifecycle */
  get nextStatus(): SkillStatus | undefined {
    switch (this.status) {
      case 'proposed': return this.scope === 'personal' ? 'generated' : 'approved';
      case 'approved': return 'generated';
      case 'generated': return 'validated';
      case 'validated': return 'active';
      default: return undefined;
    }
  }
}
