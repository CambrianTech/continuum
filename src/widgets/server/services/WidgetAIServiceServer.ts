/**
 * Widget AI Service - Server Implementation
 * 
 * Server-specific implementation extending shared AI service base.
 * Uses actual JTAG client connections to AI services.
 */

import { WidgetAIService } from '../../shared/services/ai/WidgetAIService';

export class WidgetAIServiceServer extends WidgetAIService {
  // Inherits serviceName from base class
  
  // Server implementation can connect to actual AI services
  // Base class provides mock functionality - server can override for real connections
}