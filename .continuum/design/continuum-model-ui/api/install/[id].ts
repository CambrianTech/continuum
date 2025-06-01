
// api/install/[id].ts - Next.js API-style handler to trigger install

import { exec } from 'child_process';
import { writeFileSync } from 'fs';

export default async function handler(req, res) {
  const { id } = req.query;
  if (req.method !== 'POST') return res.status(405).send('Only POST allowed');

  try {
    exec(`npx ts-node scripts/install-${id}.ts`, (err, stdout, stderr) => {
      if (err) {
        return res.status(500).json({ error: stderr });
      }
      res.status(200).json({ message: stdout || "Installed." });
    });
  } catch (e) {
    res.status(500).json({ error: e.toString() });
  }
}
