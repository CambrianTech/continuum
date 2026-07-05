/**
 * Widget AI Service - Browser Implementation
 * 
 * Browser-specific implementation extending shared AI service base.
 * Uses WebSocket connections to AI services through JTAG client.
 */

import { WidgetAIService } from '../../shared/services/ai/WidgetAIService';

export class WidgetAIServiceBrowser extends WidgetAIService {
  // Inherits serviceName from base class
  
  // Browser implementation connects to AI services via WebSocket through JTAG client
  // Base class provides mock functionality - browser can override for real connections
}