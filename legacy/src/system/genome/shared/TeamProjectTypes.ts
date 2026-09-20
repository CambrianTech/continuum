/**
 * Team Project Types — Multi-student collaborative academy training
 *
 * A team project spawns N students with different roles working on one shared
 * project. Each student gets role-specific training, then they build together.
 * The teacher grades both the overall project AND each individual's role performance.
 *
 * Architecture:
 *   1 Teacher Sentinel — orchestrates planning, training coordination, build, review
 *   N Student Sentinels — each scoped to a role, shares project workspace
 *   1 Shared Session ID — all sentinels coordinate via team-scoped events
 *   1 Academy Chat Room — ALL work posted here (the classroom/portfolio)
 *
 * Students communicate naturally via the academy chat room between pipeline steps.
 * PersonaUser autonomous loops respond to messages. Governance (proposals, voting)
 * is available for self-organization during build phases.
 *
 * @see genome/academy-team command
 * @see TeamTeacherPipeline, TeamStudentPipeline
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { AcademyConfig } from './AcademyTypes';

// ============================================================================
// Team Project Status
// ============================================================================

export type TeamProjectStatus =
  | 'pending'      // Created, sentinels not yet spawned
  | 'planning'     // Teacher decomposing project into roles + milestones
  | 'training'     // Students learning their roles via coursework
  | 'building'     // Students building the project together
  | 'reviewing'    // Teacher grading project + individual roles
  | 'complete'     // All grades assigned, project delivered
  | 'failed';      // Unrecoverable error

export const VALID_TEAM_PROJECT_STATUSES: TeamProjectStatus[] = [
  'pending', 'planning', 'training', 'building', 'reviewing', 'complete', 'failed',
];

// ============================================================================
// Team Member
// ============================================================================

/** Status of an individual team member within the project */
export type TeamMemberStatus =
  | 'pending'      // Assigned but not yet started
  | 'training'     // Learning their role
  | 'trained'      // Finished training, waiting for build phase
  | 'building'     // Actively working on project milestones
  | 'complete'     // Finished all work
  | 'failed';      // Member-level failure

/** A single member of a team project */
export interface TeamMember {
  /** Persona ID (the student) */
  personaId: UUID;

  /** Persona display name */
  personaName: string;

  /** Role within the team (e.g., "game-designer", "sprite-artist", "engineer") */
  role: string;

  /** Description of what this role does in the project */
  roleDescription: string;

  /** Sentinel handle for this member's student pipeline */
  studentHandle: string;

  /** Academy session ID for individual training tracking */
  sessionId: UUID;

  /** Current member status */
  status: TeamMemberStatus;

  // ── Grading ──

  /** Individual role performance score (0-100, assigned by teacher in review phase) */
  roleScore: number;

  /** Summary of what this member contributed to the project */
  projectContribution: string;

  // ── Training Metrics ──

  /** Number of role-specific topics passed during training */
  topicsPassed: number;

  /** Average exam score across training topics */
  averageScore: number;

  /** LoRA layer IDs produced by this member's training */
  layerIds: UUID[];
}

// ============================================================================
// Team Project Config
// ============================================================================

/** Configuration for a team project, extending AcademyConfig */
export interface TeamProjectConfig extends AcademyConfig {
  /** Maximum team members (default: 8) */
  maxMembers: number;

  /** Allow dynamic role addition mid-project (default: true) */
  dynamicRoles: boolean;

  /** Enable governance voting for team decisions (default: true) */
  enableGovernance: boolean;

  /** Number of build milestones (default: 3) */
  buildMilestones: number;
}

export const DEFAULT_TEAM_PROJECT_CONFIG: TeamProjectConfig = {
  // AcademyConfig defaults
  maxTopicAttempts: 3,
  passingScore: 70,
  epochs: 3,
  rank: 32,
  learningRate: 0.0001,
  batchSize: 4,
  examplesPerTopic: 10,
  questionsPerExam: 10,
  topicsPerSession: 3,
  // Team-specific defaults
  maxMembers: 8,
  dynamicRoles: true,
  enableGovernance: true,
  buildMilestones: 3,
};

// ============================================================================
// Team Project Phases
// ============================================================================

/** Record of a phase transition in the project lifecycle */
export interface TeamProjectPhase {
  phase: TeamProjectStatus;
  startedAt: string;
  completedAt?: string;
  notes?: string;
}

// ============================================================================
// Event Taxonomy — All events scoped by project ID
// ============================================================================

/**
 * Generate a scoped team project event name.
 * All team events follow: `team:{projectId}:{action}`
 */
export function teamEvent(projectId: string, action: string): string {
  return `team:${projectId}:${action}`;
}

/** Team project event action constants */
export const TEAM_EVENTS = {
  // Planning phase
  PROJECT_PLANNED:        'project:planned',
  ROLE_CURRICULUM_READY:  'role:curriculum:ready',

  // Training phase
  ROLE_TRAINING_DONE:     'role:training:done',
  ALL_TRAINING_DONE:      'all:training:done',

  // Build phase
  BUILD_START:            'build:start',
  BUILD_MILESTONE:        'build:milestone',
  BUILD_MILESTONE_DONE:   'build:milestone:done',
  BUILD_COMPLETE:         'build:complete',

  // Review phase
  REVIEW_READY:           'review:ready',
  REVIEW_COMPLETE:        'review:complete',

  // Dynamic membership
  MEMBER_ADDED:           'member:added',
  MEMBER_REMOVED:         'member:removed',
} as const;

export type TeamEventAction = typeof TEAM_EVENTS[keyof typeof TEAM_EVENTS];

// ============================================================================
// Pipeline Configs
// ============================================================================

/** Configuration for building the team teacher sentinel pipeline */
export interface TeamTeacherPipelineConfig {
  projectId: UUID;
  projectDescription: string;
  skill: string;
  members: TeamMember[];
  config: TeamProjectConfig;
}

/** Configuration for building a team student sentinel pipeline */
export interface TeamStudentPipelineConfig {
  projectId: UUID;
  personaId: UUID;
  personaName: string;
  role: string;
  roleDescription: string;
  baseModel: string;
  config: TeamProjectConfig;
}

// ============================================================================
// Event Payloads
// ============================================================================

export interface ProjectPlannedPayload {
  projectId: UUID;
  roles: Array<{ role: string; description: string }>;
  milestones: Array<{ name: string; description: string; tasks: string[] }>;
}

export interface RoleCurriculumReadyPayload {
  projectId: UUID;
  role: string;
  curriculum: unknown;
  topicCount: number;
}

export interface RoleTrainingDonePayload {
  projectId: UUID;
  personaId: UUID;
  personaName: string;
  role: string;
  topicsPassed: number;
  averageScore: number;
}

export interface BuildMilestonePayload {
  projectId: UUID;
  milestoneIndex: number;
  milestoneName: string;
  tasks: Array<{
    role: string;
    personaId: UUID;
    description: string;
  }>;
}

export interface BuildMilestoneDonePayload {
  projectId: UUID;
  personaId: UUID;
  role: string;
  milestoneIndex: number;
  output: string;
}

export interface ReviewCompletePayload {
  projectId: UUID;
  projectScore: number;
  projectFeedback: string;
  memberGrades: Array<{
    personaId: UUID;
    personaName: string;
    role: string;
    roleScore: number;
    feedback: string;
  }>;
}
