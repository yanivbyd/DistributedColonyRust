import { StatsRow } from './arrowLoader';

export interface ChartDataPoint {
  tick: number;
  creaturePct: number;
  emptyCellsPct: number;
}

export interface ColorCountDataPoint {
  tick: number;
  [colorKey: string]: number; // Dynamic keys for each color
}

export function transformStatsData(rows: StatsRow[]): ChartDataPoint[] {
  return rows
    .map((row) => {
      const totalCells = row.colony_width * row.colony_height;
      const creaturePct = (row.creatures_count / totalCells) * 100;
      const emptyCellsPct = 100 - creaturePct;
      
      return {
        tick: row.tick,
        creaturePct,
        emptyCellsPct,
      };
    })
    .sort((a, b) => a.tick - b.tick);
}

// Convert "R_G_B" format to hex color
function rgbStringToHex(rgbString: string): string {
  const parts = rgbString.split('_');
  if (parts.length !== 3) return '#808080'; // gray fallback
  
  const r = parseInt(parts[0], 10);
  const g = parseInt(parts[1], 10);
  const b = parseInt(parts[2], 10);
  
  if (isNaN(r) || isNaN(g) || isNaN(b)) return '#808080';
  
  return '#' + [r, g, b].map(x => {
    const hex = x.toString(16);
    return hex.length === 1 ? '0' + hex : hex;
  }).join('');
}

export function transformColorCountData(rows: StatsRow[]): {
  data: ColorCountDataPoint[];
  colors: Map<string, string>; // color name -> hex color
} {
  const colorSet = new Set<string>();
  const colorToHex = new Map<string, string>();
  
  // First pass: collect all unique colors
  for (const row of rows) {
    for (let i = 1; i <= 5; i++) {
      const colorKey = `original_color_top${i}` as keyof StatsRow;
      const color = row[colorKey];
      if (color && typeof color === 'string') {
        colorSet.add(color);
        if (!colorToHex.has(color)) {
          colorToHex.set(color, rgbStringToHex(color));
        }
      }
    }
  }
  
  // Second pass: transform data
  const data: ColorCountDataPoint[] = rows.map((row) => {
    const point: ColorCountDataPoint = {
      tick: row.tick,
    };
    
    // Add counts for each of the top 5 colors
    for (let i = 1; i <= 5; i++) {
      const colorKey = `original_color_top${i}` as keyof StatsRow;
      const countKey = `original_color_top${i}_count` as keyof StatsRow;
      const color = row[colorKey];
      const count = row[countKey];
      
      if (color && typeof color === 'string' && count !== null && count !== undefined) {
        point[color] = typeof count === 'number' ? count : Number(count);
      }
    }
    
    return point;
  }).sort((a, b) => a.tick - b.tick);
  
  return { data, colors: colorToHex };
}

