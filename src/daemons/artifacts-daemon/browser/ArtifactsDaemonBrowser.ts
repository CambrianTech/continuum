/**
 * Artifacts Daemon Browser - Client-side Filesystem Operations
 * 
 * Browser-side implementation that delegates all filesystem operations
 * to the server via router execution.
 */

import { ArtifactsDaemon, type ArtifactsPayload, type ArtifactsResult } from '../shared/ArtifactsDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';

/**
 * Browser Artifacts Daemon - Delegates to server via router
 */
export class ArtifactsDaemonBrowser extends ArtifactsDaemon {
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Browser-specific initialization
   */
  protected async initialize(): Promise<void> {
    console.log(`📁 ${this.toString()}: Browser artifacts daemon ready - delegating to server`);
  }

  /**
   * Browser delegates all filesystem operations to server
   */
  protected async handleRead(payload: ArtifactsPayload) {
    return await this.delegateToServer(payload);
  }

  protected async handleWrite(payload: ArtifactsPayload) {
    return await this.delegateToServer(payload);
  }

  protected async handleAppend(payload: ArtifactsPayload) {
    return await this.delegateToServer(payload);
  }

  protected async handleMkdir(payload: ArtifactsPayload) {
    return await this.delegateToServer(payload);
  }

  protected async handleList(payload: ArtifactsPayload) {
    return await this.delegateToServer(payload);
  }

  protected async handleStat(payload: ArtifactsPayload) {
    return await this.delegateToServer(payload);
  }

  protected async handleDelete(payload: ArtifactsPayload) {
    return await this.delegateToServer(payload);
  }

  protected async handleLoadEnvironment(payload: ArtifactsPayload) {
    return await this.delegateToServer(payload);
  }

  /**
   * Delegate operation to server via router (following DataDaemon pattern)
   */
  private async delegateToServer(payload: ArtifactsPayload): Promise<ArtifactsResult> {
    try {
      console.log(`📁 BROWSER: Delegating ${payload.operation} to server: ${payload.relativePath}`);
      
      // Create message for server artifacts daemon
      const message = JTAGMessageFactory.createRequest(
        this.context,
        'browser',
        'server/artifacts',
        payload,
        `artifacts_${payload.operation}_${Date.now()}`
      );
      
      const response = await this.router.postMessage(message);
      return response as ArtifactsResult;
      
    } catch (error: any) {
      return {
        success: false,
        error: `Browser delegation failed: ${error.message}`
      };
    }
  }
}