# Colony BI Dashboard

A React-based web dashboard for visualizing colony statistics from Arrow files.

## Setup

1. Install dependencies:
```bash
npm install
```

## Development

1. Start the Vite dev server (for React development):
```bash
npm run dev
```
This will start the dev server on http://localhost:5173 with hot module replacement.

2. In a separate terminal, start the Express server (for API and Arrow file serving):
```bash
npm run server
```
This will start the server on http://localhost:3001.

The Vite dev server is configured to proxy API and `/bi` requests to the Express server.

## Production Build

1. Build the React application:
```bash
npm run build
```

2. Start the production server:
```bash
npm run server
```

The server will serve the built React app from the `dist` directory and Arrow files from `output/bi/`.

## Features

- **Colony Selector**: Dropdown to select from available colonies (discovered from `output/bi/<colony_id>/`)
- **Creature Coverage Chart**: Stacked area chart showing:
  - Creature percentage: `(creatures_count / (colony_width * colony_height)) * 100`
  - Empty cells percentage: `100 - creature_percentage`
  - X-axis: Tick number
  - Y-axis: Percentage (0-100%)

## API Endpoints

- `GET /api/colonies` - Returns JSON list of available colony IDs
- `GET /bi/<colony_id>/stats.arrow` - Serves Arrow file for a specific colony

**Note**: The server runs on port 3001 (instead of 3000) to avoid conflicts with Grafana.

