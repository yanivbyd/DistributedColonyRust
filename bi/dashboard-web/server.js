import express from 'express';
import { readdir, stat } from 'fs/promises';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { existsSync } from 'fs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const app = express();
const PORT = process.env.PORT || 3001;

// Path to the output/bi directory (relative to project root)
const BI_DIR = join(__dirname, '..', '..', 'output', 'bi');

// Serve static files from dist directory (React build output)
app.use(express.static(join(__dirname, 'dist')));

// Serve Arrow files from output/bi directory
app.use('/bi', express.static(BI_DIR));

// API endpoint to list available colonies
app.get('/api/colonies', async (req, res) => {
  try {
    const colonies = [];
    
    if (!existsSync(BI_DIR)) {
      return res.json({ colonies: [] });
    }
    
    const entries = await readdir(BI_DIR, { withFileTypes: true });
    
    for (const entry of entries) {
      if (entry.isDirectory()) {
        const colonyPath = join(BI_DIR, entry.name);
        const statsPath = join(colonyPath, 'stats.arrow');
        
        // Check if stats.arrow exists in this directory
        if (existsSync(statsPath)) {
          colonies.push(entry.name);
        }
      }
    }
    
    res.json({ colonies: colonies.sort() });
  } catch (error) {
    console.error('Error listing colonies:', error);
    res.status(500).json({ error: 'Failed to list colonies', message: error.message });
  }
});

// Fallback: serve index.html for client-side routing
app.get('*', (req, res) => {
  const indexPath = join(__dirname, 'dist', 'index.html');
  if (existsSync(indexPath)) {
    res.sendFile(indexPath);
  } else {
    res.status(404).send('React app not built. Please run "npm run build" first.');
  }
});

app.listen(PORT, () => {
  console.log(`Server running on http://localhost:${PORT}`);
  console.log(`Serving Arrow files from: ${BI_DIR}`);
}).on('error', (err) => {
  if (err.code === 'EADDRINUSE') {
    console.error(`Port ${PORT} is already in use. Please stop the other process or use a different port.`);
  } else {
    console.error('Server error:', err);
  }
  process.exit(1);
});

