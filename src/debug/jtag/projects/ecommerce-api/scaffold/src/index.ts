/**
 * Ecommerce API — Academy Senior Project
 *
 * Build a functional ecommerce REST API from scratch.
 * Implement each milestone progressively — code accumulates.
 *
 * Seed data: 6 products across 3 categories.
 */

import express from 'express';

const app = express();
app.use(express.json());

const PORT = 3457;

// ─── Seed Data ───────────────────────────────────────────────────────────────

export const SEED_PRODUCTS = [
  { id: 'p1', name: 'Laptop Pro', price: 999.99, category: 'electronics', description: 'High-performance laptop', inStock: true },
  { id: 'p2', name: 'Wireless Mouse', price: 29.99, category: 'electronics', description: 'Ergonomic wireless mouse', inStock: true },
  { id: 'p3', name: 'Cotton T-Shirt', price: 19.99, category: 'clothing', description: 'Comfortable cotton tee', inStock: true },
  { id: 'p4', name: 'Denim Jeans', price: 49.99, category: 'clothing', description: 'Classic fit denim', inStock: true },
  { id: 'p5', name: 'TypeScript Handbook', price: 39.99, category: 'books', description: 'Complete TypeScript guide', inStock: true },
  { id: 'p6', name: 'Node.js in Action', price: 44.99, category: 'books', description: 'Practical Node.js development', inStock: false },
];

// TODO: Implement milestones here

// Export for testing
export { app };

// Start server when run directly
if (require.main === module) {
  app.listen(PORT, () => {
    console.log(`Ecommerce API running on http://localhost:${PORT}`);
  });
}
