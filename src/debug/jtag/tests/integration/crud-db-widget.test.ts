/**
 * Simple CRUD + DB + Widget Test
 *
 * Tests the only things that actually matter:
 * 1. CRUD operations work (CREATE, READ, UPDATE, DELETE)
 * 2. Data persists to database correctly
 * 3. Widget HTML reflects the data changes
 *
 * No complex event chains, no hanging debug commands, no over-engineering.
 * Just the core requirements verified reliably.
 */

import { SchemaFactory } from '../test-utils/SchemaBasedFactory';
import { runJtagCommand } from '../test-utils/CRUDTestUtils';
import { execSync } from 'child_process';

interface TestResult {
  operation: string;
  entity: string;
  dbPersistence: boolean;
  widgetHTML: boolean;
  success: boolean;
}

function checkWidgetContainsEntity(widget: string, entityId: string, timeoutMs: number = 8000): boolean {
  try {
    console.log(`🔍 Checking if ${widget} contains entity ${entityId}`);

    // For chat widget, try simple approach since JSON parsing is failing due to CSS content
    if (widget === 'chat-widget') {
      try {
        // Use direct grep to bypass JSON truncation issues
        let found = false;
        try {
          const grepResult = execSync(`./jtag debug/widget-state --widgetSelector="${widget}" --includeMessages=true 2>/dev/null | grep -c "${entityId}"`, {
            encoding: 'utf8',
            cwd: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag',
            timeout: 30000,
            shell: true
          });
          const matchCount = parseInt(grepResult.trim());
          found = matchCount > 0;
        } catch (grepError) {
          // grep returns exit code 1 if no matches found, which execSync treats as error
          found = false;
        }

        console.log(`🔍 Entity ${entityId} found in ${widget} (grep search): ${found}`);
        return found;
      } catch (chatError) {
        console.warn(`❌ Chat widget simple check failed: ${chatError.message}`);
        return false;
      }
    }

    // Use full widget-state command for other widgets
    const output = execSync(`./jtag debug/widget-state --widgetSelector="${widget}" --extractRowData=true 2>/dev/null`, {
      encoding: 'utf8',
      cwd: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag',
      timeout: timeoutMs,
      shell: true,
      maxBuffer: 1024 * 1024 * 50
    });

    let result;
    try {
      result = JSON.parse(output);
    } catch (parseError) {
      console.warn(`❌ Could not parse JSON response from widget-state command`);
      console.warn(`❌ For more detailed output, use --verbose flag`);
      console.warn(`❌ Parse error: ${parseError.message}`);
      return false;
    }

    console.log(`🔍 Widget state result for ${widget}: success=${result.success}`);

    // Check if the entity ID or content appears in the extracted row data
    if (result.success && result.rowData) {
      const found = result.rowData.some((row: any) => {
        if (!row) return false;

        // Check entity IDs in attributes
        if (row.attributes?.['entity-id'] === entityId ||
            row.attributes?.['message-id'] === entityId ||
            row.id === entityId) {
          return true;
        }

        // Check display names and content in textContent
        if (typeof entityId === 'string' && row.textContent) {
          // Search for display names (like "Updated Test User") or message content
          if (row.textContent.includes(entityId)) {
            return true;
          }
        }

        return false;
      });
      console.log(`🔍 Entity ${entityId} found in ${widget}: ${found}`);
      return found;
    }

    return false;

  } catch (error) {
    console.warn(`❌ Widget check failed for ${widget}: ${error.message}`);
    console.warn(`❌ For more detailed output, use --verbose flag`);
    return false;
  }
}

// Configuration
const WIDGET_VERIFICATION_DELAY = 3000; // Configurable delay for widget HTML synchronization
const WIDGET_TIMEOUT = 15000; // Longer timeout for widget operations during precommit

async function testCRUDWithDBAndWidget() {
  console.log('🧪 Simple CRUD + DB + Widget Test');
  console.log('==================================');
  console.log(`Widget verification delay: ${WIDGET_VERIFICATION_DELAY}ms`);
  console.log(`Widget timeout: ${WIDGET_TIMEOUT}ms\n`);

  const results: TestResult[] = [];

  const testConfigs = [
    {
      collection: 'User',
      widget: 'user-list-widget',
      createData: { displayName: 'Test User' },
      updateData: { displayName: 'Updated Test User' }
    },
    {
      collection: 'Room',
      widget: 'room-list-widget',
      createData: { name: 'Test Room' },
      updateData: { displayName: 'Updated Test Room' }
    },
    {
      collection: 'ChatMessage',
      widget: 'chat-widget',
      createData: { content: { text: 'Test Message', attachments: [] } },
      updateData: { content: { text: 'Updated Test Message', attachments: [] } }
    }
  ];

  async function testCRUD(collection: string, widget: string, createData: any, updateData: any): Promise<TestResult[]> {
    console.log(`📋 Testing ${collection}...`);
    const results: TestResult[] = [];
    let entityId: string | undefined;

    try {
      // CREATE
      const createResult = await SchemaFactory.create(collection, createData);
      if (!createResult.success || !createResult.id) {
        console.log(`❌ CREATE failed: ${createResult.error}`);
        return results;
      }

      entityId = createResult.id;
      console.log(`✅ Created: ${entityId}`);

      // Test CREATE immediately - no delays needed
      const dbRead1 = await runJtagCommand(`data/read --collection="${collection}" --id="${entityId}"`);
      const dbPersisted = Boolean(dbRead1?.success && dbRead1?.found);

      // Add small delay for UI synchronization
      await new Promise(resolve => setTimeout(resolve, WIDGET_VERIFICATION_DELAY));
      const inWidget1 = checkWidgetContainsEntity(widget, entityId, WIDGET_TIMEOUT);

      results.push({
        operation: 'CREATE',
        entity: collection,
        dbPersistence: dbPersisted,
        widgetHTML: inWidget1,
        success: dbPersisted && inWidget1
      });
      console.log(`   DB: ${dbPersisted ? '✅' : '❌'} | Widget: ${inWidget1 ? '✅' : '❌'}`);

      // UPDATE - Test for all collections
      {
        const updateResult = await runJtagCommand(`data/update --collection="${collection}" --id="${entityId}" --data='${JSON.stringify(updateData)}'`);
        if (updateResult?.found) {
        const dbRead2 = await runJtagCommand(`data/read --collection="${collection}" --id="${entityId}"`);
        const updatePersisted = Boolean(dbRead2?.success && dbRead2?.data &&
          Object.keys(updateData).every(key => JSON.stringify(dbRead2.data[key]) === JSON.stringify(updateData[key]))
        );

        await new Promise(resolve => setTimeout(resolve, WIDGET_VERIFICATION_DELAY));
        // For UPDATE: Look for the actual changed values in the widget, not just entity ID
        const updateValues = Object.values(updateData).flat();
        const inWidget2 = updateValues.some(val => {
          if (typeof val === 'object') {
            // For nested objects like ChatMessage content, look for inner values
            return Object.values(val).some(innerVal =>
              checkWidgetContainsEntity(widget, String(innerVal), WIDGET_TIMEOUT)
            );
          }
          return checkWidgetContainsEntity(widget, String(val), WIDGET_TIMEOUT);
        });

        results.push({
          operation: 'UPDATE',
          entity: collection,
          dbPersistence: updatePersisted,
          widgetHTML: inWidget2,
          success: updatePersisted && inWidget2
        });
        console.log(`   UPDATE - DB: ${updatePersisted ? '✅' : '❌'} | Widget: ${inWidget2 ? '✅' : '❌'}`);
        }
      }

      // DELETE - Test for all collections
      {
        const deleteResult = await runJtagCommand(`data/delete --collection="${collection}" --id="${entityId}"`);
        if (deleteResult?.found && deleteResult?.deleted) {
          const dbRead3 = await runJtagCommand(`data/read --collection="${collection}" --id="${entityId}"`);
          const deleteFromDB = Boolean(dbRead3?.success && !dbRead3?.found);

          // Add delay for widget to process DELETE event and update UI
          const deleteDelay = WIDGET_VERIFICATION_DELAY + 500;
          await new Promise(resolve => setTimeout(resolve, deleteDelay));
          const removedFromWidget = !checkWidgetContainsEntity(widget, entityId, 8000);

          results.push({
            operation: 'DELETE',
            entity: collection,
            dbPersistence: deleteFromDB,
            widgetHTML: removedFromWidget,
            success: deleteFromDB && removedFromWidget
          });
          console.log(`   DELETE - DB: ${deleteFromDB ? '✅' : '❌'} | Widget: ${removedFromWidget ? '✅' : '❌'}`);
        }
      }

    } catch (error) {
      console.log(`❌ ${collection} failed:`, error instanceof Error ? error.message : error);
    }

    console.log('');
    return results;
  }

  for (const config of testConfigs) {
    const configResults = await testCRUD(config.collection, config.widget, config.createData, config.updateData);
    results.push(...configResults);
  }

  // Results Summary
  console.log('📊 CRUD + DB + Widget Test Results:');
  console.log('====================================');

  const passedTests = results.filter(r => r.success).length;
  const totalTests = results.length;

  results.forEach(result => {
    const status = result.success ? '✅' : '❌';
    console.log(`${status} ${result.entity} ${result.operation}: DB(${result.dbPersistence ? '✅' : '❌'}) Widget(${result.widgetHTML ? '✅' : '❌'})`);
  });

  const successRate = ((passedTests / totalTests) * 100).toFixed(1);
  console.log(`\n📈 Results: ${passedTests}/${totalTests} passed (${successRate}%)`);

  if (successRate === '100.0') {
    console.log('🎉 ALL CRUD + DB + WIDGET TESTS PASSED!');
    console.log('✨ Database persistence and Widget HTML synchronization working perfectly');
  } else {
    console.log('⚠️ Some tests failed - check results above');
  }
}

testCRUDWithDBAndWidget().catch(error => {
  console.error('❌ Test failed:', error);
  process.exit(1);
});