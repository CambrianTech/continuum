/**
 * TrainingSession Entity - Academy GAN Training Sessions
 *
 * Manages Academy training sessions as specialized chat rooms
 * Tracks learning progress, hyperparameters, and teacher/student interactions
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// Training session types and configurations
export type SessionType = 'teacher-student' | 'peer-review' | 'self-study' | 'group-project';
export type SessionStatus = 'active' | 'paused' | 'completed' | 'archived';

export interface TrainingHyperparameters {
  learningRate: number;           // Learning rate for the session
  scoreThreshold: number;         // Minimum score to pass benchmarks
  benchmarkInterval: number;      // Messages between benchmark evaluations
  maxSessionLength: number;       // Maximum session duration in minutes
  adaptiveScoring: boolean;       // Adjust difficulty based on performance
  contextWindow: number;          // Number of previous messages to consider
}

export interface LearningObjective {
  id: UUID;
  topic: string;                  // e.g., "async-await", "closures"
  description: string;
  targetScore: number;            // 0-100 proficiency target
  currentScore?: number;          // Current assessed proficiency
  completed: boolean;
  evidence: string[];             // Message IDs that demonstrate mastery
}

export interface SessionMetrics {
  messagesExchanged: number;
  benchmarksPassed: number;
  benchmarksFailed: number;
  averageScore: number;
  timeSpent: number;              // Minutes spent in session
  objectivesCompleted: number;
  lastBenchmarkScore?: number;
  scoreHistory: Array<{
    timestamp: Date;
    score: number;
    objective: string;
  }>;
}

import {
  PrimaryField,
  TextField,
  DateField,
  EnumField,
  JsonField,
  ForeignKeyField,
  NumberField,
  BooleanField,
  TEXT_LENGTH
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';

/**
 * TrainingSession Entity - Academy training session management
 *
 * Links to a Room entity (the chat room) and adds Academy-specific training data
 */
export class TrainingSessionEntity extends BaseEntity {
  // Single source of truth for collection name
  static readonly collection = 'TrainingSession';

  @ForeignKeyField({ references: 'Room.id' })
  roomId: UUID; // The chat room where training happens

  @ForeignKeyField({ references: 'User.id' })
  teacherUserId: UUID; // Teacher persona or human instructor

  @ForeignKeyField({ references: 'User.id' })
  studentUserId: UUID; // Primary student (for 1-on-1 sessions)

  @TextField({ maxLength: TEXT_LENGTH.MEDIUM })
  sessionName: string; // "JavaScript Fundamentals", "React Patterns", etc.

  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  description?: string;

  @EnumField()
  sessionType: SessionType;

  @EnumField()
  status: SessionStatus;

  @TextField({ maxLength: TEXT_LENGTH.SHORT })
  curriculum: string; // "javascript-basics", "react-advanced", etc.

  @DateField()
  startedAt: Date;

  @DateField({ nullable: true })
  completedAt?: Date;

  @NumberField()
  plannedDuration: number; // Minutes

  @NumberField()
  actualDuration: number; // Minutes spent so far

  // Training configuration and progress
  @JsonField()
  hyperparameters: TrainingHyperparameters;

  @JsonField()
  learningObjectives: LearningObjective[];

  @JsonField()
  metrics: SessionMetrics;

  // Additional participants for group sessions
  @JsonField()
  additionalParticipants: UUID[]; // Other students or observers

  @BooleanField()
  isArchived: boolean;

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super(); // Initialize BaseEntity fields

    // Default values
    this.roomId = '' as UUID;
    this.teacherUserId = '' as UUID;
    this.studentUserId = '' as UUID;
    this.sessionName = '';
    this.sessionType = 'teacher-student';
    this.status = 'active';
    this.curriculum = 'general';
    this.startedAt = new Date();
    this.plannedDuration = 60; // 1 hour default
    this.actualDuration = 0;

    // Default hyperparameters
    this.hyperparameters = {
      learningRate: 0.1,
      scoreThreshold: 75.0,
      benchmarkInterval: 10, // Every 10 messages
      maxSessionLength: 120, // 2 hours max
      adaptiveScoring: true,
      contextWindow: 20 // Last 20 messages for context
    };

    this.learningObjectives = [];
    this.metrics = {
      messagesExchanged: 0,
      benchmarksPassed: 0,
      benchmarksFailed: 0,
      averageScore: 0,
      timeSpent: 0,
      objectivesCompleted: 0,
      scoreHistory: []
    };

    this.additionalParticipants = [];
    this.isArchived = false;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return TrainingSessionEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate training session data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields validation
    if (!this.roomId?.trim()) {
      return { success: false, error: 'TrainingSession roomId is required' };
    }

    if (!this.teacherUserId?.trim()) {
      return { success: false, error: 'TrainingSession teacherUserId is required' };
    }

    if (!this.studentUserId?.trim()) {
      return { success: false, error: 'TrainingSession studentUserId is required' };
    }

    if (!this.sessionName?.trim()) {
      return { success: false, error: 'TrainingSession sessionName is required' };
    }

    // Enum validation
    const validSessionTypes: SessionType[] = ['teacher-student', 'peer-review', 'self-study', 'group-project'];
    if (!validSessionTypes.includes(this.sessionType)) {
      return { success: false, error: `TrainingSession sessionType must be one of: ${validSessionTypes.join(', ')}` };
    }

    const validStatuses: SessionStatus[] = ['active', 'paused', 'completed', 'archived'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `TrainingSession status must be one of: ${validStatuses.join(', ')}` };
    }

    // Hyperparameters validation
    if (!this.hyperparameters) {
      return { success: false, error: 'TrainingSession hyperparameters are required' };
    }

    if (this.hyperparameters.learningRate <= 0 || this.hyperparameters.learningRate > 1) {
      return { success: false, error: 'TrainingSession learningRate must be between 0 and 1' };
    }

    if (this.hyperparameters.scoreThreshold < 0 || this.hyperparameters.scoreThreshold > 100) {
      return { success: false, error: 'TrainingSession scoreThreshold must be between 0 and 100' };
    }

    // Date validation
    if (!this.isValidDate(this.startedAt)) {
      return { success: false, error: 'TrainingSession startedAt must be a valid Date' };
    }

    if (this.completedAt && !this.isValidDate(this.completedAt)) {
      return { success: false, error: 'TrainingSession completedAt must be a valid Date' };
    }

    // Duration validation
    if (this.plannedDuration <= 0) {
      return { success: false, error: 'TrainingSession plannedDuration must be positive' };
    }

    if (this.actualDuration < 0) {
      return { success: false, error: 'TrainingSession actualDuration cannot be negative' };
    }

    return { success: true };
  }

  /**
   * Add a learning objective to the session
   */
  addLearningObjective(topic: string, description: string, targetScore: number = 80): void {
    const objective: LearningObjective = {
      id: this.generateUUID(),
      topic,
      description,
      targetScore,
      completed: false,
      evidence: []
    };

    this.learningObjectives.push(objective);
  }

  /**
   * Update progress on a learning objective
   */
  updateObjectiveProgress(objectiveId: UUID, score: number, evidenceMessageId?: string): void {
    const objective = this.learningObjectives.find(obj => obj.id === objectiveId);
    if (!objective) return;

    objective.currentScore = score;
    objective.completed = score >= objective.targetScore;

    if (evidenceMessageId && !objective.evidence.includes(evidenceMessageId)) {
      objective.evidence.push(evidenceMessageId);
    }

    // Update session metrics
    this.metrics.objectivesCompleted = this.learningObjectives.filter(obj => obj.completed).length;
    this.updateAverageScore();
  }

  /**
   * Record a benchmark result
   */
  recordBenchmark(score: number, objective: string): void {
    this.metrics.scoreHistory.push({
      timestamp: new Date(),
      score,
      objective
    });

    this.metrics.lastBenchmarkScore = score;

    if (score >= this.hyperparameters.scoreThreshold) {
      this.metrics.benchmarksPassed++;
    } else {
      this.metrics.benchmarksFailed++;
    }

    this.updateAverageScore();
  }

  /**
   * Update average score from score history
   */
  private updateAverageScore(): void {
    if (this.metrics.scoreHistory.length === 0) {
      this.metrics.averageScore = 0;
      return;
    }

    const total = this.metrics.scoreHistory.reduce((sum, entry) => sum + entry.score, 0);
    this.metrics.averageScore = Math.round((total / this.metrics.scoreHistory.length) * 100) / 100;
  }

  /**
   * Check if session should be automatically completed
   */
  shouldAutoComplete(): boolean {
    // Complete if all objectives are met
    const allObjectivesMet = this.learningObjectives.length > 0 &&
                           this.learningObjectives.every(obj => obj.completed);

    // Or if max session length exceeded
    const maxLengthExceeded = this.actualDuration >= this.hyperparameters.maxSessionLength;

    return allObjectivesMet || maxLengthExceeded;
  }

  /**
   * Generate UUID helper (would use proper UUID generator in real implementation)
   */
  private generateUUID(): UUID {
    return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}` as UUID;
  }
}