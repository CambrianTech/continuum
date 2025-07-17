/**
 * ServerAcademy - Server-specific Academy implementation
 * 
 * Handles server-side Academy operations with daemon communication
 * and process-based session management. Extends AcademyBase with server-specific overrides.
 * 
 * Following sparse override pattern: ~5-10% server-specific logic
 */

import { AcademyBase } from '../shared/AcademyBase';
import { PersonaGenome } from '../shared/AcademyTypes';
import { spawn, ChildProcess } from 'child_process';
import { promises as fs } from 'fs';
import { join } from 'path';

/**
 * Server-specific Academy implementation
 * Handles daemon communication and process-based sandboxing
 */
export abstract class ServerAcademy<TInput, TOutput> extends AcademyBase<TInput, TOutput> {
  protected sessionProcesses: Map<string, ChildProcess> = new Map();
  protected sessionDirectories: Map<string, string> = new Map();

  constructor(protected daemonClient: any) {
    super();
  }

  // ==================== SERVER-SPECIFIC OVERRIDES ====================

  /**
   * Create sandboxed session using child process
   */
  async createSandboxedSession(persona: PersonaGenome): Promise<string> {
    const sessionId = this.generateUUID();
    
    // Create temporary directory for session
    const sessionDir = join(process.cwd(), '.continuum', 'sessions', sessionId);
    await fs.mkdir(sessionDir, { recursive: true });
    this.sessionDirectories.set(sessionId, sessionDir);
    
    // Create persona context file
    const personaFile = join(sessionDir, 'persona.json');
    await fs.writeFile(personaFile, JSON.stringify(persona, null, 2));
    
    // Spawn sandboxed process
    const sessionProcess = spawn('node', [
      join(__dirname, '../../../scripts/sandbox-runner.js'),
      '--session-id', sessionId,
      '--session-dir', sessionDir,
      '--persona-file', personaFile
    ], {
      stdio: ['pipe', 'pipe', 'pipe', 'ipc'],
      env: {
        ...process.env,
        NODE_ENV: 'sandbox',
        SESSION_ID: sessionId,
        PERSONA_ID: persona.id
      }
    });
    
    this.sessionProcesses.set(sessionId, sessionProcess);
    
    // Handle process communication
    sessionProcess.on('message', (message) => {
      this.handleSessionMessage(sessionId, message);
    });
    
    sessionProcess.on('error', (error) => {
      this.logMessage(`❌ Session process error ${sessionId}: ${error.message}`);
    });
    
    sessionProcess.on('exit', (code, signal) => {
      this.logMessage(`🔚 Session process exited ${sessionId}: code=${code}, signal=${signal}`);
      this.sessionProcesses.delete(sessionId);
    });
    
    this.logMessage(`🏗️ Created server session: ${sessionId} for ${persona.name}`);
    
    return sessionId;
  }

  /**
   * Clean up process and directory
   */
  async cleanupSession(sessionId: string): Promise<void> {
    // Kill session process
    const sessionProcess = this.sessionProcesses.get(sessionId);
    if (sessionProcess) {
      sessionProcess.kill('SIGTERM');
      this.sessionProcesses.delete(sessionId);
    }
    
    // Clean up session directory
    const sessionDir = this.sessionDirectories.get(sessionId);
    if (sessionDir) {
      try {
        await fs.rm(sessionDir, { recursive: true, force: true });
      } catch (error) {
        this.logMessage(`⚠️ Failed to clean up session directory: ${error}`);
      }
      this.sessionDirectories.delete(sessionId);
    }
    
    this.logMessage(`🧹 Cleaned up server session: ${sessionId}`);
  }

  /**
   * Execute challenge in sandboxed process
   */
  async executeChallengeInSandbox(sessionId: string, challenge: any): Promise<any> {
    const sessionProcess = this.sessionProcesses.get(sessionId);
    if (!sessionProcess) {
      throw new Error(`Session process not found: ${sessionId}`);
    }
    
    return new Promise((resolve, reject) => {
      const messageHandler = (message: any) => {
        if (message.type === 'challengeResult' && message.sessionId === sessionId) {
          sessionProcess.removeListener('message', messageHandler);
          resolve(message.result);
        }
      };
      
      sessionProcess.on('message', messageHandler);
      
      // Send challenge to process
      sessionProcess.send({
        type: 'executeChallenge',
        sessionId,
        challenge
      });
      
      // Timeout after challenge time limit
      setTimeout(() => {
        sessionProcess.removeListener('message', messageHandler);
        reject(new Error('Challenge execution timeout'));
      }, challenge.timeLimit || 30000);
    });
  }

  /**
   * Send message via daemon client
   */
  async sendMessage(message: TInput): Promise<TOutput> {
    if (!this.daemonClient) {
      throw new Error('Daemon client not available');
    }
    
    try {
      const response = await this.daemonClient.sendMessage(message);
      return response;
    } catch (error) {
      this.logMessage(`❌ Daemon message failed: ${error}`);
      throw error;
    }
  }

  // ==================== SERVER-SPECIFIC METHODS ====================

  /**
   * Get server-specific status
   */
  getServerStatus(): ServerAcademyStatus {
    return {
      ...this.getAcademyStatus(),
      activeProcesses: this.sessionProcesses.size,
      sessionDirectories: this.sessionDirectories.size,
      systemInfo: {
        nodeVersion: process.version,
        platform: process.platform,
        arch: process.arch,
        memory: process.memoryUsage(),
        uptime: process.uptime()
      }
    };
  }

  /**
   * Handle messages from session processes
   */
  protected handleSessionMessage(sessionId: string, message: any): void {
    const { type, data } = message;
    
    switch (type) {
      case 'sessionInitialized':
        this.logMessage(`✅ Session initialized: ${sessionId}`);
        break;
        
      case 'challengeResult':
        this.logMessage(`📊 Challenge result from ${sessionId}: ${data.success ? 'success' : 'failure'}`);
        break;
        
      case 'sessionError':
        this.logMessage(`❌ Session error in ${sessionId}: ${data.error}`);
        break;
        
      case 'resourceUsage':
        this.monitorResourceUsage(sessionId, data);
        break;
        
      default:
        this.logMessage(`❓ Unknown session message: ${type}`);
    }
  }

  /**
   * Monitor resource usage of session processes
   */
  protected monitorResourceUsage(sessionId: string, usage: any): void {
    // Log resource usage and enforce limits
    if (usage.memory > 100 * 1024 * 1024) { // 100MB limit
      this.logMessage(`⚠️ Session ${sessionId} exceeding memory limit: ${usage.memory} bytes`);
    }
    
    if (usage.cpu > 0.8) { // 80% CPU limit
      this.logMessage(`⚠️ Session ${sessionId} exceeding CPU limit: ${usage.cpu * 100}%`);
    }
  }

  /**
   * Get session logs
   */
  async getSessionLogs(sessionId: string): Promise<string[]> {
    const sessionDir = this.sessionDirectories.get(sessionId);
    if (!sessionDir) {
      throw new Error(`Session directory not found: ${sessionId}`);
    }
    
    const logFile = join(sessionDir, 'session.log');
    
    try {
      const logContent = await fs.readFile(logFile, 'utf-8');
      return logContent.split('\n').filter(line => line.trim());
    } catch (error) {
      return [];
    }
  }

  /**
   * Get session metrics
   */
  async getSessionMetrics(sessionId: string): Promise<SessionMetrics> {
    const sessionProcess = this.sessionProcesses.get(sessionId);
    if (!sessionProcess) {
      throw new Error(`Session process not found: ${sessionId}`);
    }
    
    return new Promise((resolve, reject) => {
      const messageHandler = (message: any) => {
        if (message.type === 'metricsResponse' && message.sessionId === sessionId) {
          sessionProcess.removeListener('message', messageHandler);
          resolve(message.metrics);
        }
      };
      
      sessionProcess.on('message', messageHandler);
      
      sessionProcess.send({
        type: 'getMetrics',
        sessionId
      });
      
      setTimeout(() => {
        sessionProcess.removeListener('message', messageHandler);
        reject(new Error('Metrics request timeout'));
      }, 5000);
    });
  }

  /**
   * Override log message to include server context
   */
  protected logMessage(message: string): void {
    console.log(`[ServerAcademy] ${message}`);
  }

  /**
   * Clean up resources on destruction
   */
  destroy(): void {
    // Clean up all sessions
    for (const sessionId of this.sessionProcesses.keys()) {
      this.cleanupSession(sessionId);
    }
  }
}

// ==================== SERVER-SPECIFIC INTERFACES ====================

export interface ServerAcademyStatus {
  mode: 'idle' | 'training' | 'evaluating' | 'evolving';
  isActive: boolean;
  totalPersonas: number;
  activeSessions: number;
  uptime: number;
  activeProcesses: number;
  sessionDirectories: number;
  systemInfo: {
    nodeVersion: string;
    platform: string;
    arch: string;
    memory: NodeJS.MemoryUsage;
    uptime: number;
  };
}

export interface ServerSessionConfig {
  sandboxType: 'process' | 'container' | 'vm';
  resourceLimits: {
    memory: number;
    cpu: number;
    disk: number;
    network: boolean;
  };
  timeoutMs: number;
  allowedModules: string[];
  environment: Record<string, string>;
}

export interface SessionMetrics {
  sessionId: string;
  startTime: number;
  runtime: number;
  memoryUsage: number;
  cpuUsage: number;
  diskUsage: number;
  networkRequests: number;
  challengesCompleted: number;
  averageAccuracy: number;
}

// ==================== SERVER UTILITIES ====================

/**
 * Create secure server session configuration
 */
export function createSecureServerConfig(): ServerSessionConfig {
  return {
    sandboxType: 'process',
    resourceLimits: {
      memory: 100 * 1024 * 1024, // 100MB
      cpu: 0.5, // 50% CPU
      disk: 50 * 1024 * 1024, // 50MB
      network: false // No network access
    },
    timeoutMs: 300000, // 5 minutes
    allowedModules: [
      'fs',
      'path',
      'crypto',
      'util'
    ],
    environment: {
      NODE_ENV: 'sandbox',
      DISABLE_COLORS: 'true'
    }
  };
}

/**
 * Check system resources
 */
export function checkSystemResources(): SystemResources {
  return {
    memory: process.memoryUsage(),
    cpu: process.cpuUsage(),
    uptime: process.uptime(),
    platform: process.platform,
    arch: process.arch,
    nodeVersion: process.version
  };
}

export interface SystemResources {
  memory: NodeJS.MemoryUsage;
  cpu: NodeJS.CpuUsage;
  uptime: number;
  platform: string;
  arch: string;
  nodeVersion: string;
}