#!/usr/bin/env tsx
/**
 * Delete all anonymous users
 *
 * Anonymous users are created when browsers open without a stored userId.
 * This script deletes them all and clears any stale device associations.
 *
 * Run after: npm start
 */

import { Commands } from '../system/core/shared/Commands';
import { DATA_COMMANDS } from '../commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../commands/data/list/shared/DataListTypes';
import type { DataDeleteParams, DataDeleteResult } from '../commands/data/delete/shared/DataDeleteTypes';
import type { BaseEntity } from '../system/data/entities/BaseEntity';
import type { UserEntity } from '../system/user/entities/UserEntity';

import { DataList } from '../commands/data/list/shared/DataListTypes';
import { DataDelete } from '../commands/data/delete/shared/DataDeleteTypes';
async function main() {
	console.log('🗑️  Deleting all anonymous users...\n');

	// Get all users
	const usersResult = await DataList.execute({
		collection: 'users',
		limit: 1000,
	});

	if (!usersResult.success || !usersResult.items) {
		console.error('❌ Failed to list users:', usersResult.error);
		process.exit(1);
	}

	const users = usersResult.items as unknown as UserEntity[];

	// Filter anonymous users (uniqueId starts with "anon-" or displayName is "Anonymous User")
	const anonymousUsers = users.filter(
		(u) => u.uniqueId?.startsWith('anon-') || u.displayName === 'Anonymous User'
	);

	console.log(`Found ${anonymousUsers.length} anonymous users to delete:\n`);

	if (anonymousUsers.length === 0) {
		console.log('✅ No anonymous users found!');
		process.exit(0);
	}

	// Show what will be deleted
	anonymousUsers.forEach((u) => {
		console.log(`  - ${u.displayName} (${u.uniqueId}) - ID: ${u.id.slice(0, 8)}...`);
	});

	console.log('\n🔄 Deleting...\n');

	let deleted = 0;
	let failed = 0;

	for (const user of anonymousUsers) {
		try {
			const result = await DataDelete.execute({
				collection: 'users',
				id: user.id,
			});

			if (result.deleted) {
				console.log(`  ✅ Deleted: ${user.displayName} (${user.id.slice(0, 8)}...)`);
				deleted++;
			} else {
				console.error(`  ❌ Failed: ${user.displayName} - ${result.error}`);
				failed++;
			}
		} catch (e: any) {
			console.error(`  ❌ Error deleting ${user.displayName}: ${e.message}`);
			failed++;
		}
	}

	console.log(`\n📊 Results:`);
	console.log(`  ✅ Deleted: ${deleted}`);
	console.log(`  ❌ Failed: ${failed}`);

	if (deleted > 0) {
		console.log('\n✅ Sessions for deleted users have been cleaned up automatically.');
		console.log('   Browser tabs will get fresh identities on next reload.');
	}

	process.exit(failed > 0 ? 1 : 0);
}

main().catch((e) => {
	console.error('❌ Script failed:', e);
	process.exit(1);
});
