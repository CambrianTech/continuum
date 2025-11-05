/**
 * Academy Database Client - Generic interface to DatabaseDaemon
 * 
 * Uses the real DatabaseDaemon interface without exposing Academy-specific concepts
 * Database layer shouldn't know about LoRA - that's Academy domain knowledge
 */

export interface DatabaseRecord {
  id: string;
  table: string;
  data: any;
  created_at: Date;
  updated_at: Date;
}

/**
 * Generic Academy Database Client
 * 
 * Provides simple save/query interface without exposing Academy domain concepts
 * Database doesn't need to know about LoRA - that's Academy's business
 */
export class AcademyDatabaseClient {
  private databaseDaemon: any;

  constructor(databaseDaemon: any) {
    this.databaseDaemon = databaseDaemon;
  }

  /**
   * Save optimization record
   */
  async saveOptimizationRecord(record: any): Promise<string> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'save_record',
      data: {
        table: 'academy_optimization_records',
        data: record,
        id: record.id
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to save optimization record');
    }

    return result.data.id;
  }

  /**
   * Save persona genome
   */
  async savePersonaGenome(personaId: string, genome: any, identity: any): Promise<string> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'save_record',
      data: {
        table: 'academy_persona_genomes',
        data: { genome, identity },
        id: personaId
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to save persona genome');
    }

    return result.data.id;
  }

  /**
   * Save Academy layer (generic - Academy knows it's LoRA)
   */
  async saveLayer(layerId: string, layerData: any): Promise<string> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'save_record',
      data: {
        table: 'academy_layers',
        data: layerData,
        id: layerId
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to save layer');
    }

    return result.data.id;
  }

  /**
   * Save Academy composition (generic - Academy knows it's LoRA composition)
   */
  async saveComposition(compositionId: string, compositionData: any): Promise<string> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'save_record',
      data: {
        table: 'academy_compositions',
        data: compositionData,
        id: compositionId
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to save composition');
    }

    return result.data.id;
  }

  /**
   * Save training resource
   */
  async saveTrainingResource(resource: any): Promise<string> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'save_record',
      data: {
        table: 'academy_training_resources',
        data: resource,
        id: resource.uuid
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to save training resource');
    }

    return result.data.id;
  }

  /**
   * Save prompt binding (generic - Academy knows this binds prompts to layers)
   */
  async savePromptBinding(bindingId: string, binding: any): Promise<string> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'save_record',
      data: {
        table: 'academy_prompt_bindings',
        data: binding,
        id: bindingId
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to save prompt binding');
    }

    return result.data.id;
  }

  /**
   * Save benchmark results
   */
  async saveBenchmarkResults(batchId: string, benchmarks: any[]): Promise<string> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'save_record',
      data: {
        table: 'academy_benchmark_results',
        data: { benchmarks, timestamp: new Date() },
        id: batchId
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to save benchmark results');
    }

    return result.data.id;
  }

  /**
   * Get optimization record
   */
  async getOptimizationRecord(recordId: string): Promise<any> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'get_record',
      data: {
        table: 'academy_optimization_records',
        id: recordId
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to get optimization record');
    }

    return result.data?.data;
  }

  /**
   * Query optimization records
   */
  async queryOptimizationRecords(filters: any = {}): Promise<any[]> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'query_records',
      data: {
        table: 'academy_optimization_records',
        filters
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to query optimization records');
    }

    return result.data?.records?.map((record: any) => record.data) || [];
  }

  /**
   * Query personas by domain
   */
  async queryPersonasByDomain(domains: string[]): Promise<any[]> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'query_records',
      data: {
        table: 'academy_persona_genomes',
        filters: { domains } // Simple filter - DatabaseDaemon will handle as it can
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to query personas');
    }

    return result.data?.records?.map((record: any) => record.data) || [];
  }

  /**
   * Query Academy layers by capability (generic - Academy interprets as LoRA)
   */
  async queryLayersByCapability(capability: string): Promise<any[]> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'query_records',
      data: {
        table: 'academy_layers',
        filters: { target_capability: capability }
      }
    });

    if (!result.success) {
      throw new Error(result.error || 'Failed to query layers');
    }

    return result.data?.records?.map((record: any) => record.data) || [];
  }

  /**
   * List all records in table
   */
  async listRecords(tableName: string): Promise<any[]> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'query_records',
      data: {
        table: tableName,
        filters: {} // No filters = all records
      }
    });

    if (!result.success) {
      throw new Error(result.error || `Failed to list records from ${tableName}`);
    }

    return result.data?.records?.map((record: any) => record.data) || [];
  }

  /**
   * Delete record
   */
  async deleteRecord(tableName: string, recordId: string): Promise<boolean> {
    const result = await this.databaseDaemon.handleMessage({
      type: 'delete_record',
      data: {
        table: tableName,
        id: recordId
      }
    });

    return result.success;
  }

  /**
   * Backup Academy data
   */
  async backupAcademyData(): Promise<any> {
    const tables = [
      'academy_optimization_records',
      'academy_persona_genomes',
      'academy_layers',               // Generic - Academy knows these are LoRA layers
      'academy_compositions',         // Generic - Academy knows these are LoRA compositions  
      'academy_training_resources',
      'academy_prompt_bindings',
      'academy_benchmark_results'
    ];

    const backupResults = [];

    for (const table of tables) {
      try {
        const result = await this.databaseDaemon.handleMessage({
          type: 'backup_data',
          data: { table }
        });

        backupResults.push({
          table,
          success: result.success,
          backup_path: result.data?.backup_path,
          error: result.error
        });

      } catch (error) {
        backupResults.push({
          table,
          success: false,
          error: error instanceof Error ? error.message : String(error)
        });
      }
    }

    return {
      timestamp: new Date(),
      backups: backupResults,
      total_tables: tables.length,
      successful_backups: backupResults.filter(b => b.success).length
    };
  }

  /**
   * Get database statistics
   */
  async getAcademyStats(): Promise<any> {
    const tables = [
      'academy_optimization_records',
      'academy_persona_genomes', 
      'academy_layers',               // Generic - Academy knows these are LoRA layers
      'academy_compositions',         // Generic - Academy knows these are LoRA compositions
      'academy_training_resources',
      'academy_prompt_bindings', 
      'academy_benchmark_results'
    ];

    const stats: any = {
      timestamp: new Date(),
      tables: {}
    };

    for (const table of tables) {
      try {
        const records = await this.listRecords(table);
        stats.tables[table] = {
          record_count: records.length,
          exists: true
        };
      } catch (error) {
        stats.tables[table] = {
          record_count: 0,
          exists: false,
          error: error instanceof Error ? error.message : String(error)
        };
      }
    }

    return stats;
  }
}