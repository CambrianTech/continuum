/**
 * JTAG Message Factory - Universal Message Creation
 * 
 * Following middle-out architecture:
 * - Proper type conversions between message protocols and storage formats
 * - Universal message creation across all contexts
 * - Fixes the JTAGLogPayload → JTAGLogEntry conversion bug
 */

import {
  JTAGMessage,
  JTAGContext,
  JTAGMessageType,
  JTAGLogMessage,
  JTAGLogPayload,
  JTAGLogEntry,
  JTAGLogLevel,
  JTAGScreenshotMessage,
  JTAGScreenshotPayload,
  JTAGExecMessage,
  JTAGExecPayload,
  JTAGHealthMessage,
  JTAGHealthPayload,
  JTAGResponse
} from './JTAGCoreTypes';

import {
  JTAGDaemonMessage,
  JTAGMessagePriority
} from './JTAGDaemonTypes';

export class JTAGMessageFactory {
  
  // ============================================
  // UNIVERSAL MESSAGE CREATION
  // ============================================
  
  static createMessage<TPayload>(
    type: JTAGMessageType,
    context: JTAGContext,
    payload: TPayload,
    correlationId?: string
  ): JTAGMessage<TPayload> {
    return {
      id: this.generateMessageId(),
      type,
      context,
      timestamp: Date.now(),
      payload,
      ...(correlationId && { correlationId })
    };
  }

  static createDaemonMessage<TPayload>(
    type: JTAGMessageType,
    context: JTAGContext,
    fromDaemon: string,
    payload: TPayload,
    options: {
      toDaemon?: string;
      priority?: JTAGMessagePriority;
      requiresResponse?: boolean;
      correlationId?: string;
    } = {}
  ): JTAGDaemonMessage<TPayload> {
    const baseMessage = this.createMessage(type, context, payload, options.correlationId);
    
    return {
      ...baseMessage,
      fromDaemon,
      toDaemon: options.toDaemon,
      priority: options.priority || 'normal',
      requiresResponse: options.requiresResponse || false,
      correlationId: options.correlationId
    };
  }

  // ============================================
  // LOG MESSAGE CREATION & CONVERSION
  // ============================================
  
  static createLogMessage(
    context: JTAGContext,
    level: JTAGLogLevel,
    component: string,
    message: string,
    data?: any,
    correlationId?: string
  ): JTAGLogMessage {
    const payload: JTAGLogPayload = {
      level,
      component,
      message,
      data,
      correlationId
    };

    return this.createMessage('LOG', context, payload, correlationId) as JTAGLogMessage;
  }

  /**
   * CRITICAL FIX: Convert JTAGLogPayload to JTAGLogEntry
   * This fixes the undefined.undefined.json file naming bug
   */
  static convertLogPayloadToEntry(
    message: JTAGLogMessage,
    timestamp?: string
  ): JTAGLogEntry {
    return {
      timestamp: timestamp || new Date(message.timestamp).toISOString(),
      context: message.context,
      component: message.payload.component,
      message: message.payload.message,
      level: message.payload.level,
      data: message.payload.data,
      correlationId: message.payload.correlationId
    };
  }

  /**
   * Generate proper log file name from JTAGLogEntry
   * Format: {context}.{level}.{extension}
   * Examples: server.error.json, browser.log.txt
   */
  static generateLogFileName(
    entry: JTAGLogEntry,
    extension: 'json' | 'txt'
  ): string {
    const context = entry.context.toLowerCase();
    const level = entry.level;
    return `${context}.${level}.${extension}`;
  }

  // ============================================
  // SCREENSHOT MESSAGE CREATION
  // ============================================
  
  static createScreenshotMessage(
    context: JTAGContext,
    filename: string,
    options?: {
      selector?: string;
      screenshotOptions?: any;
      urgent?: boolean;
    }
  ): JTAGScreenshotMessage {
    const payload: JTAGScreenshotPayload = {
      filename,
      selector: options?.selector,
      options: options?.screenshotOptions,
      urgent: options?.urgent || false
    };

    return this.createMessage('SCREENSHOT', context, payload) as JTAGScreenshotMessage;
  }

  // ============================================
  // EXEC MESSAGE CREATION
  // ============================================
  
  static createExecMessage(
    context: JTAGContext,
    code: string,
    options?: {
      timeout?: number;
      environment?: Record<string, any>;
    }
  ): JTAGExecMessage {
    const payload: JTAGExecPayload = {
      code,
      timeout: options?.timeout,
      environment: options?.environment
    };

    return this.createMessage('EXEC', context, payload) as JTAGExecMessage;
  }

  // ============================================
  // HEALTH CHECK MESSAGE CREATION
  // ============================================
  
  static createHealthMessage(
    context: JTAGContext,
    options?: {
      includeMetrics?: boolean;
      includeTransports?: boolean;
      checkConnections?: boolean;
    }
  ): JTAGHealthMessage {
    const payload: JTAGHealthPayload = {
      includeMetrics: options?.includeMetrics || false,
      includeTransports: options?.includeTransports || false,
      checkConnections: options?.checkConnections || false
    };

    return this.createMessage('HEALTH_CHECK', context, payload) as JTAGHealthMessage;
  }

  // ============================================
  // RESPONSE CREATION
  // ============================================
  
  static createResponse<TData = any>(
    context: JTAGContext,
    success: boolean,
    data?: TData,
    error?: string,
    requestId?: string
  ): JTAGResponse<TData> {
    return {
      success,
      data,
      error,
      context,
      timestamp: new Date().toISOString(),
      requestId
    };
  }

  static createSuccessResponse<TData = any>(
    context: JTAGContext,
    data: TData,
    requestId?: string
  ): JTAGResponse<TData> {
    return this.createResponse(context, true, data, undefined, requestId);
  }

  static createErrorResponse(
    context: JTAGContext,
    error: string,
    requestId?: string
  ): JTAGResponse {
    return this.createResponse(context, false, undefined, error, requestId);
  }

  // ============================================
  // UTILITY METHODS
  // ============================================
  
  private static generateMessageId(): string {
    const timestamp = Date.now().toString(36);
    const random = Math.random().toString(36).substring(2);
    return `jtag_${timestamp}_${random}`;
  }

  static generateCorrelationId(): string {
    return `corr_${Date.now().toString(36)}_${Math.random().toString(36).substring(2)}`;
  }

  // ============================================
  // MESSAGE VALIDATION
  // ============================================
  
  static isValidMessage(obj: any): obj is JTAGMessage {
    return (
      obj &&
      typeof obj.id === 'string' &&
      typeof obj.type === 'string' &&
      typeof obj.context === 'string' &&
      typeof obj.timestamp === 'number' &&
      obj.payload !== undefined
    );
  }

  static isLogMessage(message: JTAGMessage): message is JTAGLogMessage {
    return message.type === 'LOG' && this.isValidLogPayload(message.payload);
  }

  static isValidLogPayload(payload: any): payload is JTAGLogPayload {
    return (
      payload &&
      typeof payload.level === 'string' &&
      typeof payload.component === 'string' &&
      typeof payload.message === 'string'
    );
  }

  // ============================================
  // DEBUGGING UTILITIES
  // ============================================
  
  static formatMessageForLog(message: JTAGMessage): string {
    const { id, type, context, timestamp } = message;
    const time = new Date(timestamp).toISOString();
    return `[${time}] ${context}:${type}:${id}`;
  }

  static extractMessageSummary(message: JTAGMessage): {
    id: string;
    type: string;
    context: string;
    timestamp: string;
    payloadType?: string;
  } {
    return {
      id: message.id,
      type: message.type,
      context: message.context,
      timestamp: new Date(message.timestamp).toISOString(),
      payloadType: typeof message.payload
    };
  }
}