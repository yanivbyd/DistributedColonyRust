import { tableFromIPC } from 'apache-arrow';

export interface StatsRow {
  tick: number;
  creatures_count: number;
  colony_width: number;
  colony_height: number;
}

export async function loadStatsArrow(url: string): Promise<StatsRow[]> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to load stats.arrow: ${response.statusText}`);
  }
  
  const arrayBuffer = await response.arrayBuffer();
  const table = tableFromIPC(arrayBuffer);
  
  const tickColumn = table.getChild('tick');
  const creaturesCountColumn = table.getChild('creatures_count');
  const colonyWidthColumn = table.getChild('colony_width');
  const colonyHeightColumn = table.getChild('colony_height');
  
  if (!tickColumn || !creaturesCountColumn || !colonyWidthColumn || !colonyHeightColumn) {
    throw new Error('Missing required columns in stats.arrow file');
  }
  
  const rows: StatsRow[] = [];
  for (let i = 0; i < table.numRows; i++) {
    const tick = tickColumn.get(i);
    const creatures_count = creaturesCountColumn.get(i);
    const colony_width = colonyWidthColumn.get(i);
    const colony_height = colonyHeightColumn.get(i);
    
    if (
      tick !== null && 
      creatures_count !== null && 
      colony_width !== null && 
      colony_height !== null
    ) {
      rows.push({
        tick: Number(tick),
        creatures_count: Number(creatures_count),
        colony_width: Number(colony_width),
        colony_height: Number(colony_height),
      });
    }
  }
  
  return rows;
}

