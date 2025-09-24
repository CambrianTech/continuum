/**
 * CRUD Test Utilities
 *
 * Reusable utilities for testing CRUD operations, event emissions,
 * and widget synchronization following elegant architecture patterns.
 */

import { execSync } from 'child_process';

// Generic entity type for any collection
export type EntityInstance = {
  id: string;
  createdAt: string;
  updatedAt: string;
  version: number;
} & Record<string, unknown>;

export interface TestResult {
  step: string;
  success: boolean;
  details?: Record<string, unknown>;
  error?: string;
}

/**
 * Execute JTAG commands and parse JSON responses
 * Enhanced with better JSON parsing and error handling
 */
export async function runJtagCommand(command: string): Promise<Record<string, unknown>> {
  try {
    const output = execSync(`./jtag ${command}`, {
      encoding: 'utf8',
      cwd: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag'
    });

    // JTAG returns JSON directly, find the JSON block
    const jsonStart = output.indexOf('{');

    if (jsonStart >= 0) {
      // Find the end by counting braces to handle nested JSON
      let braceCount = 0;
      let jsonEnd = jsonStart;

      for (let i = jsonStart; i < output.length; i++) {
        if (output[i] === '{') braceCount++;
        if (output[i] === '}') {
          braceCount--;
          if (braceCount === 0) {
            jsonEnd = i + 1;
            break;
          }
        }
      }

      if (jsonEnd > jsonStart) {
        try {
          const jsonStr = output.substring(jsonStart, jsonEnd);
          return JSON.parse(jsonStr);
        } catch (parseError) {
          console.error(`❌ JSON parse failed for command: ${command}`);
          console.error(`Parse error:`, parseError instanceof Error ? parseError.message : String(parseError));
          console.error(`JSON string (first 500 chars):`, jsonStr.substring(0, 500));

          // Return a failure object instead of throwing
          return { success: false, error: 'JSON parsing failed', command };
        }
      }
    }

    console.error(`❌ No valid JSON found in command output: ${command}`);
    console.error(`Output: ${output.substring(0, 200)}...`);
    return { success: false, error: 'No JSON found', command };

  } catch (error) {
    console.error(`❌ Command execution failed: ${command}`);
    console.error(error instanceof Error ? error.message : String(error));

    // Return failure object instead of throwing for better test control
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
      command
    };
  }
}

/**
 * Database verification utilities
 */
export class DatabaseVerifier {
  // Verify CREATE: Was row added to database?
  async verifyEntityExists(
    collection: string,
    entityId: string
  ): Promise<{ exists: boolean; data?: EntityInstance }> {
    try {
      const dbResult = await runJtagCommand(`data/read --collection=${collection} --id=${entityId}`);

      console.log(`   🔍 DB verification for ${collection}/${entityId}: success=${dbResult.success}, hasData=${Boolean(dbResult.data)}`);

      if (!dbResult.success) {
        console.log(`   ❌ DB read failed: ${dbResult.error || 'Unknown error'}`);
        return { exists: false };
      }

      const exists = Boolean(dbResult.success && dbResult.data?.id === entityId);
      return { exists, data: dbResult.data as EntityInstance };
    } catch (error) {
      console.log(`   ❌ DB verification error: ${error instanceof Error ? error.message : String(error)}`);
      return { exists: false };
    }
  }

  // Verify UPDATE: Did row change to expected values?
  async verifyEntityUpdated(
    collection: string,
    entityId: string,
    expectedChanges: Partial<EntityInstance>
  ): Promise<{ updated: boolean; actualData?: EntityInstance }> {
    const { exists, data } = await this.verifyEntityExists(collection, entityId);
    if (!exists || !data) return { updated: false };

    // Check if expected changes are reflected in the data
    const updated = Object.keys(expectedChanges).every(key =>
      data[key as keyof EntityInstance] === expectedChanges[key as keyof EntityInstance]
    );

    return { updated, actualData: data };
  }

  // Verify DELETE: Is row gone from database?
  async verifyEntityDeleted(
    collection: string,
    entityId: string
  ): Promise<{ deleted: boolean }> {
    const { exists } = await this.verifyEntityExists(collection, entityId);
    return { deleted: !exists };
  }
}

/**
 * Widget UI verification utilities
 */
export class UIVerifier {
  // Verify CREATE: Entity added to widget data and HTML?
  async verifyEntityInWidget(
    widget: string,
    entityId: string
  ): Promise<{ inData: boolean; inHTML: boolean }> {
    // Check widget data
    const widgetState = await runJtagCommand(`debug/widget-state --widgetSelector="${widget}"`);
    const widgetData = widgetState.commandResult?.state?.data ?? widgetState.commandResult?.state?.items ?? [];

    const inData = Array.isArray(widgetData)
      ? widgetData.some((item: Record<string, unknown>) => item.id === entityId)
      : JSON.stringify(widgetData).includes(entityId);

    // Check HTML DOM (note: HTML inspector requires browser environment)
    let inHTML = false;
    try {
      const htmlInspector = await runJtagCommand(`debug/html-inspector --selector="${widget}"`);
      const htmlElements = htmlInspector.commandResult?.elements ?? [];

      inHTML = htmlElements.some((element: Record<string, unknown>) =>
        element.outerHTML && String(element.outerHTML).includes(entityId)
      );
    } catch {
      // HTML inspector might not work in all environments, that's OK
      inHTML = false;
    }

    return { inData, inHTML };
  }

  // Verify UPDATE: Widget shows updated data in HTML?
  async verifyEntityUpdatedInWidget(
    widget: string,
    entityId: string,
    expectedChanges: Record<string, unknown>
  ): Promise<{ dataUpdated: boolean; htmlUpdated: boolean }> {
    const widgetState = await runJtagCommand(`debug/widget-state --widgetSelector="${widget}"`);
    const widgetData = widgetState.commandResult?.state?.data ?? widgetState.commandResult?.state?.items ?? [];

    // Find the entity in widget data and check if it has expected changes
    const entityInData = Array.isArray(widgetData)
      ? widgetData.find((item: Record<string, unknown>) => item.id === entityId)
      : null;

    const dataUpdated = entityInData && Object.keys(expectedChanges).every(key =>
      entityInData[key] === expectedChanges[key]
    );

    // Check HTML reflects the changes
    let htmlUpdated = false;
    try {
      const htmlInspector = await runJtagCommand(`debug/html-inspector --selector="${widget}"`);
      const htmlContent = JSON.stringify(htmlInspector.commandResult?.elements ?? []);

      htmlUpdated = Object.values(expectedChanges).some(value =>
        htmlContent.includes(String(value))
      );
    } catch {
      htmlUpdated = false;
    }

    return { dataUpdated: Boolean(dataUpdated), htmlUpdated };
  }

  // Verify DELETE: Entity removed from widget data and HTML?
  async verifyEntityRemovedFromWidget(
    widget: string,
    entityId: string
  ): Promise<{ removedFromData: boolean; removedFromHTML: boolean }> {
    const { inData, inHTML } = await this.verifyEntityInWidget(widget, entityId);
    return { removedFromData: !inData, removedFromHTML: !inHTML };
  }
}

/**
 * Event emission verification utilities
 */
export class EventVerifier {
  async verifyEventEmitted(
    collection: string,
    action: 'created' | 'updated' | 'deleted',
    entityId: string
  ): Promise<{ eventEmitted: boolean; eventData?: Record<string, unknown> }> {
    const eventPattern = `data:${collection}:${action}`;
    const eventLogs = await runJtagCommand(`debug/logs --filterPattern="${eventPattern}" --tailLines=20`);
    const logEntries = eventLogs.logEntries as Array<{ message?: string }> ?? [];

    const eventEmitted = logEntries.some(entry =>
      entry.message && entry.message.includes(eventPattern) && entry.message.includes(entityId)
    );

    return { eventEmitted, eventData: { eventPattern, entityId } };
  }
}

/**
 * Complete CRUD operation tester - orchestrates all verification steps
 */
export class CRUDOperationTester {
  constructor(
    private collection: string,
    private widget: string,
    private dbVerifier: DatabaseVerifier,
    private uiVerifier: UIVerifier,
    private eventVerifier: EventVerifier
  ) {}

  /**
   * Validate data against entity schema using the data/schema command
   */
  private async validateAgainstSchema(data: Record<string, unknown>): Promise<{
    valid: boolean;
    error?: string;
    validatedEntity?: Record<string, unknown>;
  }> {
    try {
      const schemaCommand = `data/schema --collection=${this.collection} --validateData='${JSON.stringify(data)}'`;
      const schemaResult = await runJtagCommand(schemaCommand);

      if (schemaResult.success && schemaResult.validation) {
        return {
          valid: schemaResult.validation.valid,
          error: schemaResult.validation.error,
          validatedEntity: schemaResult.validation.validatedEntity
        };
      } else {
        return {
          valid: false,
          error: 'Schema validation command failed'
        };
      }
    } catch (error) {
      return {
        valid: false,
        error: `Schema validation error: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  // Test complete CREATE chain: Schema Validation → Operation → Event → DB → UI
  async testCreateOperation(
    testData: Partial<EntityInstance>
  ): Promise<TestResult[]> {
    const testResults: TestResult[] = [];

    console.log(`\n🔧 Testing CREATE chain: ${this.collection}`);

    try {
      // STEP 1: Schema Validation (NEW!)
      console.log(`   🔍 Validating JSON against ${this.collection} schema...`);
      const schemaValidationResult = await this.validateAgainstSchema(testData);
      testResults.push({
        step: `${this.collection} CREATE Schema Validation`,
        success: schemaValidationResult.valid,
        details: {
          validationResult: schemaValidationResult,
          validatedData: schemaValidationResult.validatedEntity
        },
        error: schemaValidationResult.error
      });

      // Use validated data if available, otherwise original
      const dataToCreate = schemaValidationResult.validatedEntity || testData;

      if (!schemaValidationResult.valid) {
        console.log(`   ❌ Schema validation failed: ${schemaValidationResult.error}`);
        // Continue with test to show the CREATE would also fail
      }

      // STEP 2: Execute CREATE operation
      const createCommand = `data/create --collection=${this.collection} --data='${JSON.stringify(dataToCreate)}'`;
      const createResult = await runJtagCommand(createCommand);
      const entityId = createResult.id ?? createResult.data?.id;

      testResults.push({
        step: `${this.collection} CREATE Operation`,
        success: Boolean(createResult.success && entityId),
        details: { entityId, createSuccess: createResult.success }
      });

      if (!entityId) throw new Error(`CREATE failed for ${this.collection}`);

      // Verify event emission
      const eventResult = await this.eventVerifier.verifyEventEmitted(this.collection, 'created', entityId as string);
      testResults.push({
        step: `${this.collection} CREATE Event`,
        success: eventResult.eventEmitted,
        details: eventResult.eventData
      });

      // Verify database persistence
      const dbResult = await this.dbVerifier.verifyEntityExists(this.collection, entityId as string);
      testResults.push({
        step: `${this.collection} CREATE DB Persistence`,
        success: dbResult.exists,
        details: { exists: dbResult.exists }
      });

      // Verify UI update
      const uiResult = await this.uiVerifier.verifyEntityInWidget(this.widget, entityId as string);
      testResults.push({
        step: `${this.widget} CREATE UI Update`,
        success: uiResult.inData, // Don't require HTML since it may not be available in all test environments
        details: uiResult
      });

      return testResults;

    } catch (error) {
      testResults.push({
        step: `${this.collection} CREATE Chain`,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
      return testResults;
    }
  }

  // Test complete UPDATE chain: Operation → Event → DB → UI
  async testUpdateOperation(
    entityId: string,
    updates: Partial<EntityInstance>
  ): Promise<TestResult[]> {
    const testResults: TestResult[] = [];

    console.log(`\n🔧 Testing UPDATE chain: ${this.collection}`);

    try {
      // Execute UPDATE operation
      const updateCommand = `data/update --collection=${this.collection} --id=${entityId} --data='${JSON.stringify(updates)}'`;
      const updateResult = await runJtagCommand(updateCommand);

      testResults.push({
        step: `${this.collection} UPDATE Operation`,
        success: Boolean(updateResult.found),
        details: { updateFound: updateResult.found }
      });

      // Verify event emission
      const eventResult = await this.eventVerifier.verifyEventEmitted(this.collection, 'updated', entityId);
      testResults.push({
        step: `${this.collection} UPDATE Event`,
        success: eventResult.eventEmitted,
        details: eventResult.eventData
      });

      // Verify database changes
      const dbResult = await this.dbVerifier.verifyEntityUpdated(this.collection, entityId, updates);
      testResults.push({
        step: `${this.collection} UPDATE DB Changes`,
        success: dbResult.updated,
        details: { updated: dbResult.updated }
      });

      // Verify UI reflects changes
      const uiResult = await this.uiVerifier.verifyEntityUpdatedInWidget(this.widget, entityId, updates);
      testResults.push({
        step: `${this.widget} UPDATE UI Changes`,
        success: uiResult.dataUpdated, // Don't require HTML since it may not be available
        details: uiResult
      });

      return testResults;

    } catch (error) {
      testResults.push({
        step: `${this.collection} UPDATE Chain`,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
      return testResults;
    }
  }

  // Test complete DELETE chain: Operation → Event → DB → UI
  async testDeleteOperation(entityId: string): Promise<TestResult[]> {
    const testResults: TestResult[] = [];

    console.log(`\n🔧 Testing DELETE chain: ${this.collection}`);

    try {
      // Execute DELETE operation
      const deleteCommand = `data/delete --collection=${this.collection} --id=${entityId}`;
      const deleteResult = await runJtagCommand(deleteCommand);

      testResults.push({
        step: `${this.collection} DELETE Operation`,
        success: Boolean(deleteResult.success),
        details: { deleteSuccess: deleteResult.success }
      });

      // Verify event emission
      const eventResult = await this.eventVerifier.verifyEventEmitted(this.collection, 'deleted', entityId);
      testResults.push({
        step: `${this.collection} DELETE Event`,
        success: eventResult.eventEmitted,
        details: eventResult.eventData
      });

      // Verify database removal
      const dbResult = await this.dbVerifier.verifyEntityDeleted(this.collection, entityId);
      testResults.push({
        step: `${this.collection} DELETE DB Removal`,
        success: dbResult.deleted,
        details: { deleted: dbResult.deleted }
      });

      // Verify UI removal
      const uiResult = await this.uiVerifier.verifyEntityRemovedFromWidget(this.widget, entityId);
      testResults.push({
        step: `${this.widget} DELETE UI Removal`,
        success: uiResult.removedFromData, // Don't require HTML since it may not be available
        details: uiResult
      });

      return testResults;

    } catch (error) {
      testResults.push({
        step: `${this.collection} DELETE Chain`,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
      return testResults;
    }
  }
}

/**
 * Helper function to wait for async operations
 */
export function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}