/**
 * Data Marshal Command - Universal data marshalling for command chaining
 * 
 * Enables seamless data flow between commands, environments, and protocols:
 * - Base64 encoding for binary data (screenshots, files)
 * - JSON serialization for structured data 
 * - UUID correlation for tracking across command chains
 * - Promise-based chaining for complex workflows
 * - WebSocket-safe data transmission
 * 
 * Perfect for autonomous development workflows where commands need to
 * pass data to each other reliably across different execution contexts.
 */

// import { BaseCommand } from '../core/base-command/BaseCommand';
import { CommandResult } from '../../types/CommandTypes';

export interface DataMarshalOptions {
  operation: 'encode' | 'decode' | 'chain' | 'extract';
  data?: any;
  encoding?: 'base64' | 'json' | 'raw';
  correlationId?: string;
  source?: string;
  destination?: string;
  metadata?: Record<string, any>;
}

export interface MarshalledData {
  id: string;
  timestamp: string;
  encoding: string;
  originalType: string;
  size: number;
  data: string;
  metadata?: Record<string, any>;
  source?: string;
  destination?: string;
  checksum?: string;
}

export interface ChainableResult {
  marshalId: string;
  ready: boolean;
  data: any;
  next?: (command: string, params?: any) => Promise<ChainableResult>;
  extract?: (path?: string) => any;
}

export class DataMarshalCommand {
  static getDefinition() {
    return {
      name: 'data-marshal',
      description: 'Universal data marshalling for cross-environment command chaining',
      category: 'data-marshal',
      parameters: {
        operation: {
          type: 'string' as const,
          description: 'Operation to perform: encode, decode, chain, extract',
          required: true,
          enum: ['encode', 'decode', 'chain', 'extract']
        },
        data: {
          type: 'object' as const,
          description: 'Data to marshal/unmarshal',
          required: false
        },
        encoding: {
          type: 'string' as const,
          description: 'Encoding format: base64, json, raw',
          required: false,
          default: 'json',
          enum: ['base64', 'json', 'raw']
        },
        correlationId: {
          type: 'string' as const,
          description: 'UUID for tracking data across command chains',
          required: false
        },
        source: {
          type: 'string' as const,
          description: 'Source command or context',
          required: false
        },
        destination: {
          type: 'string' as const,
          description: 'Destination command or context',
          required: false
        },
        metadata: {
          type: 'object' as const,
          description: 'Additional metadata to attach',
          required: false
        }
      },
      examples: [
        {
          description: 'Encode screenshot data for transmission',
          command: 'data-marshal --operation=encode --data="<binary>" --encoding=base64 --source=screenshot'
        },
        {
          description: 'Chain JS execution result to next command',
          command: 'data-marshal --operation=chain --data="{...}" --destination=widget-inspect'
        },
        {
          description: 'Extract specific field from marshalled data',
          command: 'data-marshal --operation=extract --data="<marshalled>" --metadata=\'{"path":"widgets[0].tagName"}\''
        }
      ]
    };
  }

  static async execute(params: any, _context?: any): Promise<CommandResult> {
    try {
      const options = DataMarshalCommand.parseParams(params);
      
      const {
        operation,
        data,
        encoding = 'json',
        correlationId,
        source,
        destination,
        metadata = {}
      } = options;

      switch (operation) {
        case 'encode':
          return DataMarshalCommand.encodeData(data, encoding, {
            ...(correlationId && { correlationId }), 
            ...(source && { source }), 
            ...(destination && { destination }), 
            metadata
          });

        case 'decode':
          return DataMarshalCommand.decodeData(data, {
            ...(correlationId && { correlationId }), 
            ...(source && { source }), 
            ...(destination && { destination }), 
            metadata
          });

        case 'chain':
          return DataMarshalCommand.chainData(data, {
            ...(correlationId && { correlationId }), 
            ...(source && { source }), 
            ...(destination && { destination }), 
            metadata
          });

        case 'extract':
          return DataMarshalCommand.extractData(data, metadata);

        default:
          throw new Error(`Unsupported operation: ${operation}`);
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      return {
        success: false,
        error: `Data marshalling failed: ${errorMessage}`,
        data: {
          timestamp: new Date().toISOString()
        }
      };
    }
  }

  /**
   * Encode data for cross-environment transmission
   */
  private static async encodeData(
    data: any, 
    encoding: string, 
    options: { correlationId?: string; source?: string; destination?: string; metadata?: any }
  ): Promise<CommandResult> {
    const marshalId = options.correlationId || 
      `marshal-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

    let encodedData: string;
    let originalType: string;
    let size: number;

    try {
      switch (encoding) {
        case 'base64':
          if (typeof data === 'string') {
            encodedData = Buffer.from(data, 'utf-8').toString('base64');
            originalType = 'string';
          } else if (Buffer.isBuffer(data)) {
            encodedData = data.toString('base64');
            originalType = 'buffer';
          } else {
            // Convert to JSON first, then base64
            const jsonString = JSON.stringify(data);
            encodedData = Buffer.from(jsonString, 'utf-8').toString('base64');
            originalType = 'object';
          }
          size = encodedData.length;
          break;

        case 'json':
          encodedData = JSON.stringify(data, null, 2);
          originalType = typeof data;
          size = encodedData.length;
          break;

        case 'raw':
          encodedData = String(data);
          originalType = typeof data;
          size = encodedData.length;
          break;

        default:
          throw new Error(`Unsupported encoding: ${encoding}`);
      }

      const marshalled: MarshalledData = {
        id: marshalId,
        timestamp: new Date().toISOString(),
        encoding,
        originalType,
        size,
        data: encodedData,
        metadata: options.metadata,
        ...(options.source && { source: options.source }),
        ...(options.destination && { destination: options.destination }),
        checksum: DataMarshalCommand.calculateChecksum(encodedData)
      };

      return {
        success: true,
        data: {
          marshalled,
          marshalId,
          encoding,
          size,
          ready: true
        },
        message: `Data marshalled successfully [${marshalId}] (${size} bytes, ${encoding})`
      };

    } catch (error) {
      throw new Error(`Encoding failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * Decode previously marshalled data
   */
  private static async decodeData(
    marshalledData: any,
    _options: { correlationId?: string; source?: string; destination?: string; metadata?: any }
  ): Promise<CommandResult> {
    try {
      let marshalled: MarshalledData;

      // Handle different input formats
      if (typeof marshalledData === 'string') {
        try {
          marshalled = JSON.parse(marshalledData);
        } catch {
          throw new Error('Invalid marshalled data format');
        }
      } else if (marshalledData && typeof marshalledData === 'object') {
        marshalled = marshalledData;
      } else {
        throw new Error('Marshalled data must be object or JSON string');
      }

      // Verify checksum if present
      if (marshalled.checksum) {
        const calculatedChecksum = DataMarshalCommand.calculateChecksum(marshalled.data);
        if (calculatedChecksum !== marshalled.checksum) {
          throw new Error('Data integrity check failed - checksum mismatch');
        }
      }

      let decodedData: any;

      switch (marshalled.encoding) {
        case 'base64':
          const buffer = Buffer.from(marshalled.data, 'base64');
          
          if (marshalled.originalType === 'buffer') {
            decodedData = buffer;
          } else if (marshalled.originalType === 'string') {
            decodedData = buffer.toString('utf-8');
          } else {
            // Try to parse as JSON
            try {
              const jsonString = buffer.toString('utf-8');
              decodedData = JSON.parse(jsonString);
            } catch {
              decodedData = buffer.toString('utf-8');
            }
          }
          break;

        case 'json':
          decodedData = JSON.parse(marshalled.data);
          break;

        case 'raw':
          decodedData = marshalled.data;
          break;

        default:
          throw new Error(`Unsupported encoding: ${marshalled.encoding}`);
      }

      return {
        success: true,
        data: {
          decoded: decodedData,
          marshalId: marshalled.id,
          originalMetadata: marshalled.metadata,
          source: marshalled.source,
          destination: marshalled.destination,
          decodedAt: new Date().toISOString()
        },
        message: `Data decoded successfully [${marshalled.id}]`
      };

    } catch (error) {
      throw new Error(`Decoding failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * Create chainable data for command composition
   */
  private static async chainData(
    data: any,
    options: { correlationId?: string; source?: string; destination?: string; metadata?: any }
  ): Promise<CommandResult> {
    const chainId = options.correlationId || 
      `chain-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

    // Create a chainable result with promise-based next() method
    const chainable: ChainableResult = {
      marshalId: chainId,
      ready: true,
      data: data,
      
      // Placeholder for actual command execution
      next: async (command: string, params?: any) => {
        // This would integrate with the command system to execute the next command
        // For now, return a mock chainable result
        return {
          marshalId: `${chainId}-next`,
          ready: true,
          data: { nextCommand: command, params },
          extract: (path?: string) => path ? DataMarshalCommand.extractPath(data, path) : data
        };
      },

      extract: (path?: string) => {
        return path ? DataMarshalCommand.extractPath(data, path) : data;
      }
    };

    return {
      success: true,
      data: {
        chainable,
        chainId,
        ready: true,
        source: options.source,
        destination: options.destination
      },
      message: `Chainable data created [${chainId}]`
    };
  }

  /**
   * Extract specific data from marshalled payload
   */
  private static async extractData(data: any, metadata: any): Promise<CommandResult> {
    try {
      const path = metadata?.path;
      const extracted = path ? DataMarshalCommand.extractPath(data, path) : data;

      return {
        success: true,
        data: {
          extracted,
          path,
          extractedAt: new Date().toISOString()
        },
        message: path ? `Extracted data at path: ${path}` : 'Full data extracted'
      };

    } catch (error) {
      throw new Error(`Extraction failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * Extract data using dot notation path (e.g., "widgets[0].tagName")
   */
  private static extractPath(obj: any, path: string): any {
    try {
      return path.split('.').reduce((current, key) => {
        // Handle array notation like "widgets[0]"
        if (key.includes('[') && key.includes(']')) {
          const [arrayKey, indexStr] = key.split('[');
          const index = parseInt(indexStr.replace(']', ''), 10);
          return current[arrayKey][index];
        }
        return current[key];
      }, obj);
    } catch {
      return undefined;
    }
  }

  /**
   * Calculate simple checksum for data integrity
   */
  private static calculateChecksum(data: string): string {
    let hash = 0;
    for (let i = 0; i < data.length; i++) {
      const char = data.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash; // Convert to 32-bit integer
    }
    return Math.abs(hash).toString(16);
  }

  /**
   * Parse command parameters from various input formats
   */
  protected static parseParams(params: any): DataMarshalOptions {
    // Handle command line args (--key=value format)
    if (params && params.args && Array.isArray(params.args)) {
      const result: any = {};
      for (const arg of params.args) {
        if (typeof arg === 'string' && arg.startsWith('--')) {
          const [key, value] = arg.split('=', 2);
          const cleanKey = key.replace('--', '');
          
          // Parse special values
          if (value === 'true') result[cleanKey] = true;
          else if (value === 'false') result[cleanKey] = false;
          else if (cleanKey === 'metadata' || cleanKey === 'data') {
            try {
              result[cleanKey] = JSON.parse(value);
            } catch {
              result[cleanKey] = value;
            }
          } else {
            result[cleanKey] = value;
          }
        }
      }
      return result;
    }
    
    // Handle object parameters
    if (typeof params === 'object' && params !== null) {
      return params;
    }
    
    // Handle JSON string
    if (typeof params === 'string') {
      try {
        return JSON.parse(params);
      } catch {
        return { operation: 'encode', data: params };
      }
    }
    
    return { operation: 'encode' };
  }
}

export default DataMarshalCommand;