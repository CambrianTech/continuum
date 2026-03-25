/**
 * TeamProjectEntity — Persistent state for a collaborative team training project
 *
 * Tracks a multi-student project where each persona has a different role.
 * The teacher decomposes the project into roles, trains each student for
 * their role, then orchestrates collaborative building and dual grading
 * (project quality + individual role performance).
 *
 * Lifecycle: pending → planning → training → building → reviewing → complete | failed
 *
 * @see TeamProjectTypes for type definitions
 * @see TeamTeacherPipeline for teacher orchestration
 * @see TeamStudentPipeline for per-role student execution
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  EnumField,
  JsonField,
  TEXT_LENGTH,
} from '../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../data/entities/BaseEntity';
import { LOCAL_MODELS } from '@system/shared/Constants';
import type {
  TeamProjectStatus,
  TeamMember,
  TeamMemberStatus,
  TeamProjectConfig,
  TeamProjectPhase,
} from '../shared/TeamProjectTypes';
import {
  VALID_TEAM_PROJECT_STATUSES,
  DEFAULT_TEAM_PROJECT_CONFIG,
} from '../shared/TeamProjectTypes';

export class TeamProjectEntity extends BaseEntity {
  static readonly collection = 'academy_team_projects';

  /** What the team is building (e.g., "side-scrolling game with mushroom people") */
  @TextField({ maxLength: TEXT_LENGTH.LONG })
  projectDescription: string;

  /** Skill domain (e.g., "game-development", "web-app", "music-production") */
  @TextField({ index: true })
  skill: string;

  /** Base model for training */
  @TextField()
  baseModel: string;

  /** Current project lifecycle status */
  @EnumField({ index: true })
  status: TeamProjectStatus;

  /** Sentinel handle for the teacher pipeline */
  @TextField({ nullable: true })
  teacherHandle?: string;

  /** Recipe ID that defines roles (if created from a recipe) */
  @TextField({ nullable: true })
  recipeId?: string;

  /** All team members with their roles and state */
  @JsonField()
  members: TeamMember[];

  /** Shared workspace directory for the project output */
  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  workspaceDir?: string;

  /** Project configuration */
  @JsonField()
  config: TeamProjectConfig;

  /** Phase transition history */
  @JsonField()
  phases: TeamProjectPhase[];

  /** Current phase index */
  @NumberField()
  currentPhase: number;

  /** Overall project score (0-100, assigned in review phase) */
  @NumberField()
  projectScore: number;

  /** Teacher's feedback on the overall project */
  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  projectFeedback?: string;

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();
    this.projectDescription = '';
    this.skill = '';
    this.baseModel = LOCAL_MODELS.DEFAULT;
    this.status = 'pending';
    this.members = [];
    this.config = { ...DEFAULT_TEAM_PROJECT_CONFIG };
    this.phases = [];
    this.currentPhase = 0;
    this.projectScore = 0;
  }

  get collection(): string {
    return TeamProjectEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.projectDescription?.trim()) {
      return { success: false, error: 'projectDescription is required' };
    }
    if (!this.skill?.trim()) {
      return { success: false, error: 'skill is required' };
    }
    if (!VALID_TEAM_PROJECT_STATUSES.includes(this.status)) {
      return { success: false, error: `status must be one of: ${VALID_TEAM_PROJECT_STATUSES.join(', ')}` };
    }
    if (this.members.length < 1) {
      return { success: false, error: 'team project requires at least 1 member' };
    }
    if (this.members.length > this.config.maxMembers) {
      return { success: false, error: `team has ${this.members.length} members, max is ${this.config.maxMembers}` };
    }
    for (const m of this.members) {
      if (!m.personaId?.trim()) {
        return { success: false, error: 'each member must have a personaId' };
      }
      if (!m.role?.trim()) {
        return { success: false, error: 'each member must have a role' };
      }
    }
    return { success: true };
  }

  // ── Lifecycle Methods ───────────────────────────────────────────────

  /** Transition to planning phase */
  markPlanning(teacherHandle: string): void {
    this.status = 'planning';
    this.teacherHandle = teacherHandle;
    this.phases.push({ phase: 'planning', startedAt: new Date().toISOString() });
  }

  /** Add a new team member dynamically */
  addMember(member: TeamMember): void {
    if (this.members.length >= this.config.maxMembers) {
      throw new Error(`Team already has ${this.members.length} members (max: ${this.config.maxMembers})`);
    }
    this.members.push(member);
  }

  /** Transition to training phase */
  markTraining(): void {
    this.status = 'training';
    this._closeCurrentPhase();
    this.phases.push({ phase: 'training', startedAt: new Date().toISOString() });
  }

  /** Transition to building phase */
  markBuilding(): void {
    this.status = 'building';
    this._closeCurrentPhase();
    this.phases.push({ phase: 'building', startedAt: new Date().toISOString() });
  }

  /** Transition to review phase */
  markReviewing(): void {
    this.status = 'reviewing';
    this._closeCurrentPhase();
    this.phases.push({ phase: 'reviewing', startedAt: new Date().toISOString() });
  }

  /** Complete the project with final scores */
  markComplete(projectScore: number, projectFeedback: string): void {
    this.status = 'complete';
    this.projectScore = projectScore;
    this.projectFeedback = projectFeedback;
    this._closeCurrentPhase();
    this.phases.push({ phase: 'complete', startedAt: new Date().toISOString() });
  }

  /** Grade an individual member's role performance */
  gradeMember(personaId: UUID, roleScore: number, contribution: string): void {
    const member = this.members.find(m => m.personaId === personaId);
    if (!member) throw new Error(`Member ${personaId} not found in team`);
    member.roleScore = roleScore;
    member.projectContribution = contribution;
    member.status = 'complete';
  }

  /** Mark as failed */
  markFailed(notes?: string): void {
    this.status = 'failed';
    this._closeCurrentPhase();
    this.phases.push({ phase: 'failed', startedAt: new Date().toISOString(), notes });
  }

  /** Update a member's status */
  updateMemberStatus(personaId: UUID, status: TeamMemberStatus): void {
    const member = this.members.find(m => m.personaId === personaId);
    if (!member) throw new Error(`Member ${personaId} not found in team`);
    member.status = status;
  }

  // ── Private Helpers ─────────────────────────────────────────────────

  private _closeCurrentPhase(): void {
    const current = this.phases[this.phases.length - 1];
    if (current && !current.completedAt) {
      current.completedAt = new Date().toISOString();
    }
    this.currentPhase = this.phases.length;
  }
}
