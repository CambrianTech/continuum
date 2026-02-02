/**
 * Challenge Definitions - Progressive coding challenges for AI training
 *
 * Challenges are ordered by difficulty:
 * 1-2: Beginner (single file, simple operations)
 * 3-4: Intermediate (multi-file, dependency chains)
 * 5-6: Advanced (bug tracing, multi-agent)
 * 7:   Expert (architecture migration)
 *
 * Each definition contains everything needed to create a CodingChallengeEntity.
 */

import type { ChallengeDifficulty, ChallengeCategory } from '../../data/entities/CodingChallengeEntity';

export interface ChallengeDefinition {
  name: string;
  sequenceNumber: number;
  difficulty: ChallengeDifficulty;
  category: ChallengeCategory;
  description: string;
  setupFiles: Record<string, string>;
  expectedOutcome: string;
  evaluationCriteria: string[];
  expectedFiles?: Record<string, string>;
  timeLimitMs: number;
  toolCallLimit: number;
}

// ────────────────────────────────────────────────────────────
// Challenge 1: Single-File Function Addition (Beginner)
// ────────────────────────────────────────────────────────────

export const CHALLENGE_1_FUNCTION_ADD: ChallengeDefinition = {
  name: 'Add a function to a single file',
  sequenceNumber: 1,
  difficulty: 'beginner',
  category: 'single-file',
  description: `Read the file "math-utils.ts" and add a new exported function called "factorial" that computes the factorial of a non-negative integer. It should throw an error for negative inputs. Do not modify the existing functions.`,
  setupFiles: {
    'math-utils.ts': `/**
 * Math utility functions
 */

export function add(a: number, b: number): number {
  return a + b;
}

export function multiply(a: number, b: number): number {
  return a * b;
}

export function isPrime(n: number): boolean {
  if (n < 2) return false;
  for (let i = 2; i * i <= n; i++) {
    if (n % i === 0) return false;
  }
  return true;
}
`,
  },
  expectedOutcome: 'The file math-utils.ts should contain the original three functions plus a new "factorial" function that handles edge cases correctly.',
  evaluationCriteria: [
    'factorial function is exported and correctly computes factorial for n >= 0',
    'factorial(0) returns 1 (base case)',
    'factorial throws an error for negative input',
    'Existing functions (add, multiply, isPrime) are unchanged',
    'Code follows the existing style (TypeScript, exported functions)',
  ],
  expectedFiles: {
    'math-utils.ts': `/**
 * Math utility functions
 */

export function add(a: number, b: number): number {
  return a + b;
}

export function multiply(a: number, b: number): number {
  return a * b;
}

export function isPrime(n: number): boolean {
  if (n < 2) return false;
  for (let i = 2; i * i <= n; i++) {
    if (n % i === 0) return false;
  }
  return true;
}

export function factorial(n: number): number {
  if (n < 0) throw new Error('factorial requires a non-negative integer');
  if (n === 0 || n === 1) return 1;
  let result = 1;
  for (let i = 2; i <= n; i++) {
    result *= i;
  }
  return result;
}
`,
  },
  timeLimitMs: 60_000,
  toolCallLimit: 8,
};

// ────────────────────────────────────────────────────────────
// Challenge 2: Create File + Unit Test (Beginner)
// ────────────────────────────────────────────────────────────

export const CHALLENGE_2_FILE_PLUS_TEST: ChallengeDefinition = {
  name: 'Create a function and its unit test',
  sequenceNumber: 2,
  difficulty: 'beginner',
  category: 'multi-file',
  description: `Create two files:
1. "string-utils.ts" — export a function "slugify(input: string): string" that converts a string to a URL-safe slug (lowercase, spaces/special chars replaced with hyphens, no leading/trailing hyphens, no consecutive hyphens).
2. "string-utils.test.ts" — write tests for slugify covering: basic conversion, multiple spaces, special characters, leading/trailing spaces, empty string, already-slugified input.

Use simple assertion statements (no test framework needed). Each test should be a function that throws if the assertion fails.`,
  setupFiles: {
    'README.md': '# String Utils\n\nCreate string-utils.ts and string-utils.test.ts as described.',
  },
  expectedOutcome: 'Two files created: string-utils.ts with a working slugify function, and string-utils.test.ts with comprehensive tests.',
  evaluationCriteria: [
    'string-utils.ts exports a slugify function with correct signature',
    'slugify converts "Hello World" to "hello-world"',
    'slugify handles special characters (e.g., "Hello, World!" → "hello-world")',
    'slugify removes leading/trailing hyphens',
    'slugify collapses consecutive hyphens',
    'string-utils.test.ts exists and contains meaningful test cases',
    'Tests cover edge cases: empty string, already-slugified, special chars',
  ],
  timeLimitMs: 90_000,
  toolCallLimit: 12,
};

// ────────────────────────────────────────────────────────────
// Challenge 3: Multi-File Refactor (Intermediate)
// ────────────────────────────────────────────────────────────

export const CHALLENGE_3_EXTRACT_SHARED: ChallengeDefinition = {
  name: 'Extract shared utility from duplicate code',
  sequenceNumber: 3,
  difficulty: 'intermediate',
  category: 'refactoring',
  description: `Three files (user-service.ts, order-service.ts, product-service.ts) each contain a duplicated "formatCurrency" function with identical logic. Refactor by:
1. Creating a new "shared/format-utils.ts" that exports the single canonical formatCurrency function
2. Updating all three service files to import from shared/format-utils.ts instead of having their own copy
3. Do NOT change the function's behavior — only move it

The three service files also have other functions that should NOT be changed.`,
  setupFiles: {
    'user-service.ts': `import type { User } from './types';

function formatCurrency(amount: number, currency: string = 'USD'): string {
  return new Intl.NumberFormat('en-US', { style: 'currency', currency }).format(amount);
}

export function getUserBalance(user: User): string {
  return formatCurrency(user.balance);
}

export function getUserSummary(user: User): string {
  return \`\${user.name}: \${formatCurrency(user.balance)}\`;
}
`,
    'order-service.ts': `import type { Order } from './types';

function formatCurrency(amount: number, currency: string = 'USD'): string {
  return new Intl.NumberFormat('en-US', { style: 'currency', currency }).format(amount);
}

export function getOrderTotal(order: Order): string {
  const total = order.items.reduce((sum, item) => sum + item.price * item.quantity, 0);
  return formatCurrency(total, order.currency);
}

export function formatOrderLine(name: string, price: number): string {
  return \`\${name}: \${formatCurrency(price)}\`;
}
`,
    'product-service.ts': `import type { Product } from './types';

function formatCurrency(amount: number, currency: string = 'USD'): string {
  return new Intl.NumberFormat('en-US', { style: 'currency', currency }).format(amount);
}

export function getProductPrice(product: Product): string {
  return formatCurrency(product.price, product.currency);
}

export function getDiscountedPrice(product: Product, discount: number): string {
  const discounted = product.price * (1 - discount);
  return formatCurrency(discounted, product.currency);
}
`,
    'types.ts': `export interface User {
  name: string;
  balance: number;
}

export interface OrderItem {
  name: string;
  price: number;
  quantity: number;
}

export interface Order {
  items: OrderItem[];
  currency: string;
}

export interface Product {
  name: string;
  price: number;
  currency: string;
}
`,
  },
  expectedOutcome: 'A new shared/format-utils.ts file containing the single formatCurrency function, with all three service files updated to import from it. No behavior changes.',
  evaluationCriteria: [
    'shared/format-utils.ts exists and exports formatCurrency',
    'formatCurrency function signature and behavior is preserved exactly',
    'user-service.ts imports formatCurrency from shared/format-utils',
    'order-service.ts imports formatCurrency from shared/format-utils',
    'product-service.ts imports formatCurrency from shared/format-utils',
    'No duplicate formatCurrency definitions remain in any service file',
    'All other functions in service files are unchanged',
    'types.ts is unmodified',
  ],
  timeLimitMs: 120_000,
  toolCallLimit: 15,
};

// ────────────────────────────────────────────────────────────
// Challenge 4: Add Feature with Types + Handler + Test (Intermediate)
// ────────────────────────────────────────────────────────────

export const CHALLENGE_4_FEATURE_ENDPOINT: ChallengeDefinition = {
  name: 'Add a feature across types, handler, and test',
  sequenceNumber: 4,
  difficulty: 'intermediate',
  category: 'feature',
  description: `Add a "search" feature to the existing todo application:
1. Add a "SearchParams" interface to types.ts with fields: query (string), completed (boolean | undefined)
2. Add a "searchTodos" function to todo-service.ts that filters todos by title substring match and optional completed status
3. Add tests for searchTodos in todo-service.test.ts covering: text search, completed filter, combined search+filter, empty results, empty query returns all

Follow the existing patterns in each file.`,
  setupFiles: {
    'types.ts': `export interface Todo {
  id: string;
  title: string;
  completed: boolean;
  createdAt: number;
}

export interface CreateTodoParams {
  title: string;
}
`,
    'todo-service.ts': `import type { Todo, CreateTodoParams } from './types';

const todos: Todo[] = [];
let nextId = 1;

export function createTodo(params: CreateTodoParams): Todo {
  const todo: Todo = {
    id: String(nextId++),
    title: params.title,
    completed: false,
    createdAt: Date.now(),
  };
  todos.push(todo);
  return todo;
}

export function getTodos(): Todo[] {
  return [...todos];
}

export function completeTodo(id: string): Todo | undefined {
  const todo = todos.find(t => t.id === id);
  if (todo) todo.completed = true;
  return todo;
}
`,
    'todo-service.test.ts': `import { createTodo, getTodos, completeTodo } from './todo-service';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(\`Assertion failed: \${message}\`);
}

// Test createTodo
const todo = createTodo({ title: 'Buy groceries' });
assert(todo.title === 'Buy groceries', 'createTodo should set title');
assert(todo.completed === false, 'createTodo should default to incomplete');
assert(typeof todo.id === 'string', 'createTodo should assign string id');

// Test getTodos
const allTodos = getTodos();
assert(allTodos.length >= 1, 'getTodos should return created todos');

// Test completeTodo
const completed = completeTodo(todo.id);
assert(completed?.completed === true, 'completeTodo should mark as complete');

console.log('All tests passed!');
`,
  },
  expectedOutcome: 'types.ts has SearchParams, todo-service.ts has searchTodos function, todo-service.test.ts has comprehensive search tests.',
  evaluationCriteria: [
    'SearchParams interface added to types.ts with correct fields',
    'searchTodos function added to todo-service.ts',
    'searchTodos filters by title substring (case-insensitive)',
    'searchTodos filters by completed status when provided',
    'searchTodos returns all when query is empty and no filter',
    'Tests added for all search scenarios',
    'Existing code in all three files is preserved',
  ],
  timeLimitMs: 120_000,
  toolCallLimit: 15,
};

// ────────────────────────────────────────────────────────────
// Challenge 5: Bug Fix by Call Chain Tracing (Advanced)
// ────────────────────────────────────────────────────────────

export const CHALLENGE_5_BUG_FIX: ChallengeDefinition = {
  name: 'Find and fix a bug by tracing the call chain',
  sequenceNumber: 5,
  difficulty: 'advanced',
  category: 'bug-fix',
  description: `There is a bug in the discount calculation system. When a user applies a percentage discount coupon, the final price is sometimes negative for large discounts.

The bug report: "When I apply a 50% discount coupon to a $10 item, the price shows as -$5.00 instead of $5.00"

Trace through the code files to find the root cause and fix it. The bug is in the calculation logic, not the formatting. Hint: look at how the discount is applied.`,
  setupFiles: {
    'cart.ts': `import { applyDiscount } from './pricing';
import type { CartItem, Coupon } from './types';

export function calculateCartTotal(items: CartItem[], coupon?: Coupon): number {
  let total = items.reduce((sum, item) => sum + item.price * item.quantity, 0);
  if (coupon) {
    total = applyDiscount(total, coupon);
  }
  return total;
}
`,
    'pricing.ts': `import type { Coupon } from './types';
import { calculatePercentageDiscount, calculateFixedDiscount } from './discounts';

export function applyDiscount(total: number, coupon: Coupon): number {
  switch (coupon.type) {
    case 'percentage':
      return calculatePercentageDiscount(total, coupon.value);
    case 'fixed':
      return calculateFixedDiscount(total, coupon.value);
    default:
      return total;
  }
}
`,
    'discounts.ts': `/**
 * Calculate the discounted price after applying a percentage discount.
 * @param total - Original price
 * @param percentage - Discount percentage (e.g., 50 for 50%)
 * @returns Discounted price
 */
export function calculatePercentageDiscount(total: number, percentage: number): number {
  // BUG: subtracts percentage as a raw number instead of computing the percentage
  const discount = percentage;
  return total - discount;
}

/**
 * Calculate the discounted price after applying a fixed amount discount.
 * @param total - Original price
 * @param amount - Fixed discount amount
 * @returns Discounted price (minimum 0)
 */
export function calculateFixedDiscount(total: number, amount: number): number {
  return Math.max(0, total - amount);
}
`,
    'types.ts': `export interface CartItem {
  name: string;
  price: number;
  quantity: number;
}

export interface Coupon {
  code: string;
  type: 'percentage' | 'fixed';
  value: number;
}
`,
  },
  expectedOutcome: 'The calculatePercentageDiscount function should compute the actual percentage discount (total * percentage / 100) and ensure the result is non-negative.',
  evaluationCriteria: [
    'Root cause identified: calculatePercentageDiscount subtracts raw percentage instead of computing percentage of total',
    'Fix: discount = total * (percentage / 100)',
    'Result includes Math.max(0, ...) to prevent negative prices',
    'Only discounts.ts is modified (other files have no bugs)',
    'calculateFixedDiscount is unchanged (it already works correctly)',
    'Function signature and JSDoc are preserved',
  ],
  expectedFiles: {
    'discounts.ts': `/**
 * Calculate the discounted price after applying a percentage discount.
 * @param total - Original price
 * @param percentage - Discount percentage (e.g., 50 for 50%)
 * @returns Discounted price
 */
export function calculatePercentageDiscount(total: number, percentage: number): number {
  const discount = total * (percentage / 100);
  return Math.max(0, total - discount);
}

/**
 * Calculate the discounted price after applying a fixed amount discount.
 * @param total - Original price
 * @param amount - Fixed discount amount
 * @returns Discounted price (minimum 0)
 */
export function calculateFixedDiscount(total: number, amount: number): number {
  return Math.max(0, total - amount);
}
`,
  },
  timeLimitMs: 120_000,
  toolCallLimit: 15,
};

// ────────────────────────────────────────────────────────────
// All challenges in order
// ────────────────────────────────────────────────────────────

export const ALL_CHALLENGES: ChallengeDefinition[] = [
  CHALLENGE_1_FUNCTION_ADD,
  CHALLENGE_2_FILE_PLUS_TEST,
  CHALLENGE_3_EXTRACT_SHARED,
  CHALLENGE_4_FEATURE_ENDPOINT,
  CHALLENGE_5_BUG_FIX,
];
