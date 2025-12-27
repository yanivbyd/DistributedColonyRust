import { StatsRow } from './arrowLoader';

export interface ChartDataPoint {
  tick: number;
  creaturePct: number;
  emptyCellsPct: number;
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

