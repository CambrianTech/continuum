/**
 * Academy Create Session Command Types
 *
 * Creates new Academy training sessions with proper room setup and hyperparameters
 * Links training sessions to chat rooms for seamless Academy/chat integration
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface AcademyCreateSessionParams {
  sessionName: string;            // "JavaScript Fundamentals"
  curriculum: string;             // "javascript-basics"
  teacherUserId: UUID;           // Teacher persona or human
  studentUserId: UUID;           // Primary student
  description?: string;

  // Session configuration
  plannedDuration?: number;       // Minutes, default 60
  sessionType?: 'teacher-student' | 'peer-review' | 'self-study' | 'group-project';

  // Learning objectives
  objectives?: Array<{
    topic: string;
    description: string;
    targetScore?: number;         // Default 80
  }>;

  // Hyperparameters
  hyperparameters?: {
    learningRate?: number;        // Default 0.1
    scoreThreshold?: number;      // Default 75
    benchmarkInterval?: number;   // Default 10 messages
    maxSessionLength?: number;    // Default 120 minutes
    adaptiveScoring?: boolean;    // Default true
    contextWindow?: number;       // Default 20 messages
  };

  // Additional participants for group sessions
  additionalParticipants?: UUID[];

  // Room configuration
  createRoom?: boolean;           // Create new room or use existing
  roomName?: string;             // Custom room name
  existingRoomId?: UUID;         // Use existing room
}

export interface AcademyCreateSessionResult {
  sessionId: UUID;
  roomId: UUID;
  sessionName: string;
  roomName: string;
  teacherName: string;
  studentName: string;
  curriculum: string;
  plannedDuration: number;
  objectivesCount: number;
  hyperparameters: {
    learningRate: number;
    scoreThreshold: number;
    benchmarkInterval: number;
    adaptiveScoring: boolean;
  };
  status: 'created' | 'error';
  message: string;
}