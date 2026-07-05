/**
 * URL Shortener API — Academy Project
 *
 * Build a URL shortener service with Express.
 * Start here and implement the milestones one at a time.
 */

import express from 'express';

const app = express();
app.use(express.json());

const PORT = 3456;

// TODO: Implement milestones here

// Export for testing
export { app };

// Start server when run directly
if (require.main === module) {
  app.listen(PORT, () => {
    console.log(`URL Shortener running on http://localhost:${PORT}`);
  });
}
