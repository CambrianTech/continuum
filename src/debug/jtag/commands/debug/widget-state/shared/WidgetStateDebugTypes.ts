/**
 * Widget State Debug Command Types
 * 
 * Command to deeply inspect widget state and data connectivity
 */

export interface WidgetStateDebugParams {
  widgetSelector?: string;
  includeMessages?: boolean;
  testDataConnectivity?: boolean;
  roomId?: string;
}

export interface WidgetStateDebugResult {
  success: boolean;
  widgetFound: boolean;
  widgetPath: string;
  methods: string[];
  state: any;
  messages: any[];
  dataTest?: {
    rawData: any;
    filteredData: any;
    filterResults: string[];
  };
  connectivity: {
    hasJtagOperation: boolean;
    windowJtag: boolean;
    windowWidgetDaemon: boolean;
  };
  error?: string;
}