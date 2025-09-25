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

function runCommand(command: string, timeoutMs: number = 3000): any {
  try {
    const output = execSync(`./jtag ${command}`, {
      encoding: 'utf8',
      cwd: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag',
      timeout: timeoutMs
    });

    const jsonStart = output.lastIndexOf('{');
    if (jsonStart >= 0) {
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

      return JSON.parse(output.substring(jsonStart, jsonEnd));
    }
    return null;
  } catch (error) {
    return null;
  }
}

async function testCRUDWithDBAndWidget() {
  console.log('🧪 Simple CRUD + DB + Widget Test');
  console.log('==================================\n');

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
      updateData: { name: 'Updated Test Room' }
    },
    {
      collection: 'ChatMessage',
      widget: 'chat-widget',
      createData: { content: { text: 'Test Message', attachments: [] } },
      updateData: { content: { text: 'Updated Test Message', attachments: [] } }
    }
  ];

  for (const config of testConfigs) {
    console.log(`📋 Testing ${config.collection}...`);
    let entityId: string | undefined;

    try {
      // 1. CREATE
      const createResult = await SchemaFactory.create(config.collection, config.createData);
      if (!createResult.success || !createResult.id) {
        console.log(`❌ CREATE failed: ${createResult.error}`);
        continue;
      }

      entityId = createResult.id;
      console.log(`✅ Created: ${entityId}`);

      // Longer delay for CREATE to persist (fixed race condition)
      await new Promise(resolve => setTimeout(resolve, 500));

      // Verify DB persistence for CREATE
      const dbRead1 = await runJtagCommand(`data/read --collection="${config.collection}" --id="${entityId}"`);
      const dbPersisted = Boolean(dbRead1?.success && dbRead1?.found);

      // Verify Widget HTML for CREATE
      const widgetHTML1 = runCommand(`debug/html-inspector --selector="${config.widget}"`, 5000);
      const htmlContent1 = JSON.stringify(widgetHTML1?.commandResult?.elements || []);
      const inHTML1 = htmlContent1.includes(entityId);

      results.push({
        operation: 'CREATE',
        entity: config.collection,
        dbPersistence: dbPersisted,
        widgetHTML: inHTML1,
        success: dbPersisted && inHTML1
      });

      console.log(`   DB: ${dbPersisted ? '✅' : '❌'} | Widget: ${inHTML1 ? '✅' : '❌'}`);

      // 2. UPDATE
      const updateResult = await runJtagCommand(`data/update --collection="${config.collection}" --id="${entityId}" --data='${JSON.stringify(config.updateData)}'`);
      const updateSuccess = Boolean(updateResult?.found);

      if (updateSuccess) {
        // Verify DB persistence for UPDATE
        const dbRead2 = await runJtagCommand(`data/read --collection="${config.collection}" --id="${entityId}"`);
        const updatePersisted = Boolean(dbRead2?.success && dbRead2?.data &&
          Object.keys(config.updateData).every(key =>
            JSON.stringify(dbRead2.data[key]) === JSON.stringify(config.updateData[key as keyof typeof config.updateData])
          )
        );

        // Verify Widget HTML for UPDATE
        const widgetHTML2 = runCommand(`debug/html-inspector --selector="${config.widget}"`, 5000);
        const htmlContent2 = JSON.stringify(widgetHTML2?.commandResult?.elements || []);
        const updateValues = Object.values(config.updateData).flat();
        const inHTML2 = updateValues.some(val =>
          typeof val === 'object' ?
            Object.values(val).some(v => htmlContent2.includes(String(v))) :
            htmlContent2.includes(String(val))
        );

        results.push({
          operation: 'UPDATE',
          entity: config.collection,
          dbPersistence: updatePersisted,
          widgetHTML: inHTML2,
          success: updatePersisted && inHTML2
        });

        console.log(`   UPDATE - DB: ${updatePersisted ? '✅' : '❌'} | Widget: ${inHTML2 ? '✅' : '❌'}`);
      } else {
        results.push({
          operation: 'UPDATE',
          entity: config.collection,
          dbPersistence: false,
          widgetHTML: false,
          success: false
        });
        console.log(`   UPDATE - ❌ Operation failed`);
      }

      // 3. DELETE
      const deleteResult = await runJtagCommand(`data/delete --collection="${config.collection}" --id="${entityId}"`);
      const deleteSuccess = Boolean(deleteResult?.found && deleteResult?.deleted);

      if (deleteSuccess) {
        // Verify DB persistence for DELETE (entity should be gone)
        const dbRead3 = await runJtagCommand(`data/read --collection="${config.collection}" --id="${entityId}"`);
        const deleteFromDB = Boolean(dbRead3?.success && !dbRead3?.found);

        // Verify Widget HTML for DELETE (entityId should be gone)
        const widgetHTML3 = runCommand(`debug/html-inspector --selector="${config.widget}"`, 5000);
        const htmlContent3 = JSON.stringify(widgetHTML3?.commandResult?.elements || []);
        const removedFromHTML = !htmlContent3.includes(entityId);

        results.push({
          operation: 'DELETE',
          entity: config.collection,
          dbPersistence: deleteFromDB,
          widgetHTML: removedFromHTML,
          success: deleteFromDB && removedFromHTML
        });

        console.log(`   DELETE - DB: ${deleteFromDB ? '✅' : '❌'} | Widget: ${removedFromHTML ? '✅' : '❌'}`);
      } else {
        results.push({
          operation: 'DELETE',
          entity: config.collection,
          dbPersistence: false,
          widgetHTML: false,
          success: false
        });
        console.log(`   DELETE - ❌ Operation failed`);
      }

    } catch (error) {
      console.log(`❌ ${config.collection} failed:`, error instanceof Error ? error.message : error);
    }

    console.log('');
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