/**
 * Academy Sessions Debug Command Types
 *
 * Debug command for inspecting and managing Academy training sessions
 * Helps debug training progress, hyperparameters, and session metrics
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface AcademySessionsDebugParams {
  sessionId?: UUID;               // Inspect specific session
  teacherId?: UUID;               // Filter by teacher
  studentId?: UUID;               // Filter by student
  curriculum?: string;            // Filter by curriculum
  status?: 'active' | 'paused' | 'completed' | 'archived';
  showMetrics?: boolean;          // Include detailed metrics
  showObjectives?: boolean;       // Include learning objectives
  exportFormat?: 'json' | 'table' | 'summary';
}

export interface SessionSummary {
  sessionId: UUID;
  sessionName: string;
  curriculum: string;
  teacherName: string;
  studentName: string;
  status: string;
  progress: {
    duration: string;              // "45m / 60m planned"
    objectivesCompleted: string;   // "3/5 objectives"
    averageScore: number;
    lastBenchmark: number | null;
  };
  hyperparameters: {
    learningRate: number;
    scoreThreshold: number;
    adaptiveScoring: boolean;
  };
}

export interface AcademySessionsDebugResult {
  totalSessions: number;
  activeSessions: number;
  completedSessions: number;
  sessions: SessionSummary[];
  curricula: {
    [curriculum: string]: {
      sessions: number;
      averageScore: number;
      completionRate: number;
    };
  };
  teachers: {
    [teacherId: string]: {
      name: string;
      activeSessions: number;
      averageStudentScore: number;
    };
  };
  performance: {
    highPerformers: UUID[];        // Students with >90% avg score
    strugglingStudents: UUID[];    // Students with <60% avg score
    stalledSessions: UUID[];       // Sessions inactive >24h
  };
  recommendations?: string[];
}