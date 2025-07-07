#!/usr/bin/env tsx
/**
 * Debug Widget Discovery - Find out why widgets aren't being discovered
 */

import { DiscoverWidgetsCommand } from '../../../commands/ui/discover-widgets/DiscoverWidgetsCommand';

async function debugWidgetDiscovery() {
  console.log('🔍 Debug: Testing widget discovery directly...');
  
  try {
    const result = await DiscoverWidgetsCommand.execute({});
    console.log('🔍 Debug: DiscoverWidgetsCommand result:', JSON.stringify(result, null, 2));
  } catch (error) {
    console.error('🔍 Debug: DiscoverWidgetsCommand failed:', error);
  }
}

debugWidgetDiscovery();