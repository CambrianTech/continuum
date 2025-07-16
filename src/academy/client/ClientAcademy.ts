/**
 * ClientAcademy - Client-specific Academy implementation
 * 
 * Handles browser-based Academy operations with WebSocket communication
 * and DOM-specific session management. Extends AcademyBase with client-specific overrides.
 * 
 * Following sparse override pattern: ~5-10% client-specific logic
 */

import { AcademyBase } from '../shared/AcademyBase';
import { PersonaGenome } from '../shared/AcademyTypes';

/**
 * Client-specific Academy implementation
 * Handles WebSocket communication and browser-based sandboxing
 */
export abstract class ClientAcademy<TInput, TOutput> extends AcademyBase<TInput, TOutput> {
  protected websocket: WebSocket | null = null;
  protected sessionWindows: Map<string, Window> = new Map();

  constructor(protected websocketUrl: string) {
    super();
    this.initializeWebSocket();
  }

  // ==================== CLIENT-SPECIFIC OVERRIDES ====================

  /**
   * Create sandboxed session using iframe or popup window
   */
  async createSandboxedSession(persona: PersonaGenome): Promise<string> {
    const sessionId = this.generateUUID();
    
    // Create sandboxed iframe for persona session
    const iframe = document.createElement('iframe');
    iframe.id = `session-${sessionId}`;
    iframe.src = 'about:blank';
    iframe.style.display = 'none';
    iframe.sandbox = 'allow-scripts allow-same-origin';
    
    document.body.appendChild(iframe);
    
    // Initialize session in iframe
    if (iframe.contentWindow) {
      this.sessionWindows.set(sessionId, iframe.contentWindow);
      
      // Initialize persona context in iframe
      iframe.contentWindow.postMessage({
        type: 'initializePersonaSession',
        sessionId,
        persona: persona
      }, '*');
    }
    
    this.logMessage(`🏗️ Created client session: ${sessionId} for ${persona.identity.name}`);
    
    return sessionId;
  }

  /**
   * Clean up iframe session
   */
  async cleanupSession(sessionId: string): Promise<void> {
    const iframe = document.getElementById(`session-${sessionId}`);
    if (iframe) {
      iframe.remove();
    }
    
    this.sessionWindows.delete(sessionId);
    
    this.logMessage(`🧹 Cleaned up client session: ${sessionId}`);
  }

  /**
   * Execute challenge in sandboxed iframe
   */
  async executeChallengeInSandbox(sessionId: string, challenge: any): Promise<any> {
    const sessionWindow = this.sessionWindows.get(sessionId);
    if (!sessionWindow) {
      throw new Error(`Session window not found: ${sessionId}`);
    }
    
    return new Promise((resolve, reject) => {
      const messageHandler = (event: MessageEvent) => {
        if (event.data.type === 'challengeResult' && event.data.sessionId === sessionId) {
          window.removeEventListener('message', messageHandler);
          resolve(event.data.result);
        }
      };
      
      window.addEventListener('message', messageHandler);
      
      // Send challenge to iframe
      sessionWindow.postMessage({
        type: 'executeChallenge',
        sessionId,
        challenge
      }, '*');
      
      // Timeout after challenge time limit
      setTimeout(() => {
        window.removeEventListener('message', messageHandler);
        reject(new Error('Challenge execution timeout'));
      }, challenge.timeLimit || 30000);
    });
  }

  /**
   * Send message via WebSocket
   */
  async sendMessage(message: TInput): Promise<TOutput> {
    if (!this.websocket || this.websocket.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket not connected');
    }
    
    return new Promise((resolve, reject) => {
      const messageId = this.generateUUID();
      
      const messageHandler = (event: MessageEvent) => {
        const data = JSON.parse(event.data);
        if (data.id === messageId) {
          this.websocket?.removeEventListener('message', messageHandler);
          resolve(data.response);
        }
      };
      
      this.websocket.addEventListener('message', messageHandler);
      
      this.websocket.send(JSON.stringify({
        id: messageId,
        message
      }));
      
      // Timeout after 30 seconds
      setTimeout(() => {
        this.websocket?.removeEventListener('message', messageHandler);
        reject(new Error('Message timeout'));
      }, 30000);
    });
  }

  // ==================== CLIENT-SPECIFIC METHODS ====================

  /**
   * Initialize WebSocket connection
   */
  protected initializeWebSocket(): void {
    try {
      this.websocket = new WebSocket(this.websocketUrl);
      
      this.websocket.onopen = () => {
        this.logMessage('🔌 WebSocket connected');
      };
      
      this.websocket.onclose = () => {
        this.logMessage('❌ WebSocket disconnected');
        // Attempt to reconnect after delay
        setTimeout(() => this.initializeWebSocket(), 5000);
      };
      
      this.websocket.onerror = (error) => {
        this.logMessage(`❌ WebSocket error: ${error}`);
      };
      
    } catch (error) {
      this.logMessage(`❌ WebSocket initialization failed: ${error}`);
    }
  }

  /**
   * Get client-specific status
   */
  getClientStatus(): ClientAcademyStatus {
    return {
      ...this.getAcademyStatus(),
      websocketStatus: this.websocket?.readyState || WebSocket.CLOSED,
      activeSandboxes: this.sessionWindows.size,
      browserInfo: {
        userAgent: navigator.userAgent,
        language: navigator.language,
        platform: navigator.platform
      }
    };
  }

  /**
   * Handle window messages from sandboxed sessions
   */
  protected handleSessionMessage(event: MessageEvent): void {
    const { type, sessionId, data } = event.data;
    
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
        
      default:
        this.logMessage(`❓ Unknown session message: ${type}`);
    }
  }

  /**
   * Override log message to include client context
   */
  protected logMessage(message: string): void {
    console.log(`[ClientAcademy] ${message}`);
  }

  /**
   * Clean up resources on destruction
   */
  destroy(): void {
    // Clean up all sessions
    for (const sessionId of this.sessionWindows.keys()) {
      this.cleanupSession(sessionId);
    }
    
    // Close WebSocket
    if (this.websocket) {
      this.websocket.close();
    }
  }
}

// ==================== CLIENT-SPECIFIC INTERFACES ====================

export interface ClientAcademyStatus {
  mode: 'idle' | 'training' | 'evaluating' | 'evolving';
  isActive: boolean;
  totalPersonas: number;
  activeSessions: number;
  uptime: number;
  websocketStatus: number;
  activeSandboxes: number;
  browserInfo: {
    userAgent: string;
    language: string;
    platform: string;
  };
}

export interface ClientSessionConfig {
  sandboxType: 'iframe' | 'popup' | 'worker';
  allowedOrigins: string[];
  sessionTimeout: number;
  resourceLimits: {
    memory: number;
    cpu: number;
    storage: number;
  };
}

// ==================== CLIENT UTILITIES ====================

/**
 * Check if browser supports required features
 */
export function checkBrowserSupport(): boolean {
  return !!(
    window.WebSocket &&
    window.postMessage &&
    document.createElement('iframe').sandbox &&
    window.localStorage
  );
}

/**
 * Get optimal sandbox type for current browser
 */
export function getOptimalSandboxType(): 'iframe' | 'popup' | 'worker' {
  // Check for Web Workers support
  if (typeof Worker !== 'undefined') {
    return 'worker';
  }
  
  // Check for popup blocker
  const testPopup = window.open('about:blank', '_blank', 'width=1,height=1');
  if (testPopup) {
    testPopup.close();
    return 'popup';
  }
  
  // Fallback to iframe
  return 'iframe';
}

/**
 * Create secure sandbox configuration
 */
export function createSecureSandboxConfig(): ClientSessionConfig {
  return {
    sandboxType: getOptimalSandboxType(),
    allowedOrigins: [window.location.origin],
    sessionTimeout: 300000, // 5 minutes
    resourceLimits: {
      memory: 50 * 1024 * 1024, // 50MB
      cpu: 0.5, // 50% CPU
      storage: 10 * 1024 * 1024 // 10MB
    }
  };
}