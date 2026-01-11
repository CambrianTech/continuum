/**
 * Quick diagnostic to check Events subscription state
 * Run with: npx tsx scripts/check-subscriptions.ts
 */
import { Events } from '../system/core/shared/Events';

// Access private static Maps using type assertion
const eventsClass = Events as any;

console.log('=== Events Subscription Diagnostic ===');
console.log('');

// Check elegantSubscriptions
const elegantSubs = eventsClass.elegantSubscriptions;
if (elegantSubs) {
  console.log(`elegantSubscriptions: ${elegantSubs.size} subscriptions`);
  elegantSubs.forEach((sub: any, id: string) => {
    const pattern = sub.originalPattern || String(sub.pattern);
    console.log(`  - ${id}: ${pattern}`);
  });
} else {
  console.log('elegantSubscriptions: not initialized');
}

console.log('');

// Check exactMatchSubscriptions
const exactSubs = eventsClass.exactMatchSubscriptions;
if (exactSubs) {
  console.log(`exactMatchSubscriptions: ${exactSubs.size} subscriptions`);
  exactSubs.forEach((sub: any, id: string) => {
    console.log(`  - ${id}: ${sub.eventName}`);
    if (sub.filter && sub.filter.where) {
      console.log(`    filter: ${JSON.stringify(sub.filter.where)}`);
    }
  });
} else {
  console.log('exactMatchSubscriptions: not initialized');
}

console.log('');

// Check wildcardSubscriptions
const wildcardSubs = eventsClass.wildcardSubscriptions;
if (wildcardSubs) {
  console.log(`wildcardSubscriptions: ${wildcardSubs.size} subscriptions`);
  wildcardSubs.forEach((sub: any, id: string) => {
    console.log(`  - ${id}: ${sub.eventName}`);
  });
} else {
  console.log('wildcardSubscriptions: not initialized');
}

console.log('');
console.log('=== End Diagnostic ===');
