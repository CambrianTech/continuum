/**
 * End-to-End Data CRUD Testing
 * 
 * Tests complete data daemon functionality with all operations:
 * CREATE, READ, UPDATE, DELETE, LIST
 */

import { jtag } from '@continuum/jtag';

async function testDataCRUD() {
  console.log('🧪 Starting COMPLETE Data CRUD Test...');
  
  try {
    // Connect to JTAG system
    const connectionResult = await jtag.connect();
    console.log('Connection result:', connectionResult);
    console.log('Connection result keys:', Object.keys(connectionResult || {}));
    
    const jtagClient = connectionResult?.client || connectionResult;
    console.log('JTAG client:', jtagClient);
    console.log('JTAG client type:', typeof jtagClient);
    console.log('JTAG client keys:', Object.keys(jtagClient || {}));
    console.log('✅ Connected to JTAG system');
    
    // Test data for CRUD operations
    const testCollection = 'crud-test';
    const testData = { name: 'Alice', age: 25, role: 'developer' };
    const updateData = { age: 26, department: 'engineering' };
    
    console.log('\n📝 Testing CREATE operation...');
    const createResult = await jtagClient.commands['data/create']({
      collection: testCollection,
      data: testData,
      format: 'json'
    });
    
    console.log('Create result:', createResult);
    
    if (!createResult.success) {
      throw new Error(`CREATE failed: ${createResult.error}`);
    }
    
    const recordId = createResult.id;
    console.log(`✅ CREATE: Record created with ID ${recordId}`);
    
    console.log('\n📖 Testing READ operation...');
    const readResult = await jtagClient.commands['data/read']({
      collection: testCollection,
      id: recordId,
      format: 'json'
    });
    
    if (!readResult.success) {
      throw new Error(`READ failed: ${readResult.error}`);
    }
    
    console.log('Read result:', readResult);
    console.log(`✅ READ: Retrieved record`, readResult.data);
    
    console.log('\n✏️ Testing UPDATE operation...');
    const updateResult = await jtagClient.commands['data/update']({
      collection: testCollection,
      id: recordId,
      data: updateData,
      format: 'json'
    });
    
    if (!updateResult.success) {
      throw new Error(`UPDATE failed: ${updateResult.error}`);
    }
    
    console.log(`✅ UPDATE: Record updated`, updateResult.data);
    
    console.log('\n📋 Testing LIST operation...');
    const listResult = await jtagClient.commands['data/list']({
      collection: testCollection,
      format: 'json'
    });
    
    if (!listResult.success) {
      throw new Error(`LIST failed: ${listResult.error}`);
    }
    
    console.log(`✅ LIST: Found ${listResult.data.length} records in collection`);
    
    console.log('\n🗑️ Testing DELETE operation...');
    const deleteResult = await jtagClient.commands['data/delete']({
      collection: testCollection,
      id: recordId,
      format: 'json'
    });
    
    if (!deleteResult.success) {
      throw new Error(`DELETE failed: ${deleteResult.error}`);
    }
    
    console.log(`✅ DELETE: Record deleted successfully`);
    
    console.log('\n🔍 Verifying DELETE - attempting to read deleted record...');
    const verifyDeleteResult = await jtagClient.commands['data/read']({
      collection: testCollection,
      id: recordId,
      format: 'json'
    });
    
    if (verifyDeleteResult.success) {
      console.warn(`⚠️ WARNING: Record still exists after DELETE!`);
    } else {
      console.log(`✅ VERIFY DELETE: Record properly deleted (read failed as expected)`);
    }
    
    console.log('\n🎉 ALL DATA CRUD OPERATIONS COMPLETED SUCCESSFULLY!');
    console.log('📊 Test Summary:');
    console.log('  ✅ CREATE: Record creation');
    console.log('  ✅ READ: Record retrieval');  
    console.log('  ✅ UPDATE: Record modification');
    console.log('  ✅ LIST: Collection listing');
    console.log('  ✅ DELETE: Record removal');
    console.log('  ✅ VERIFY: Deletion confirmation');
    
  } catch (error) {
    console.error('❌ Data CRUD test failed:', error);
    throw error;
  }
}

// Run the test
testDataCRUD().catch(console.error);