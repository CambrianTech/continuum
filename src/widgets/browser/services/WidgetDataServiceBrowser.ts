/**
 * Widget Data Service - Browser Implementation
 * 
 * Browser-specific implementation extending shared data service base.
 * Uses browser storage APIs (localStorage, sessionStorage, IndexedDB).
 */

import { WidgetDataService } from '../../shared/services/data/WidgetDataService';

export class WidgetDataServiceBrowser extends WidgetDataService {
  // Inherits serviceName from base class
  
  // Browser implementation can use localStorage, sessionStorage, IndexedDB
  // Base class provides mock functionality - browser can override for real storage
}