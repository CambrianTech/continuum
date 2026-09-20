import express from 'express';
import http from 'http';

// TODO: Implement the full-stack integration application
// Milestones:
//   1. E-commerce foundation (products, auth, cart)
//   2. Checkout and inventory (orders, stock tracking)
//   3. WebSocket real-time layer (hello/welcome, /ws/stats)
//   4. Collaborative whiteboard (join, draw, sync, clear, history)
//   5. Analytics pipeline (view/cart/order tracking, /analytics/*)
//   6. Dashboard and integration (/dashboard, /dashboard/activity, WS push)

export const app = express();

app.use(express.json());

app.get('/health', (_req, res) => {
  res.json({ status: 'ok' });
});

// Milestones 1-2: Add product catalog, auth, cart, orders, and inventory here

// Milestone 3: Attach WebSocket server to httpServer
// Switch export from app.listen() to exporting httpServer directly

// Milestones 4-6: Add whiteboard, analytics, and dashboard routes

// Export the HTTP server so tests can call server.listen(PORT)
export const server = http.createServer(app);
