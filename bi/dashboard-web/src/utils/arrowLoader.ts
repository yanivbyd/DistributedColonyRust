import { tableFromIPC } from 'apache-arrow';

export interface StatsRow {
  tick: number;
  creatures_count: number;
  colony_width: number;
  colony_height: number;
  original_color_top1?: string | null;
  original_color_top1_count?: number | null;
  original_color_top2?: string | null;
  original_color_top2_count?: number | null;
  original_color_top3?: string | null;
  original_color_top3_count?: number | null;
  original_color_top4?: string | null;
  original_color_top4_count?: number | null;
  original_color_top5?: string | null;
  original_color_top5_count?: number | null;
  creature_size_mean?: number | null;
  creature_size_avg?: number | null;
  can_kill_true_fraction?: number | null;
  can_kill_average?: number | null;
  can_move_true_fraction?: number | null;
  can_move_average?: number | null;
  health_mean?: number | null;
  health_avg?: number | null;
  age_mean?: number | null;
  age_avg?: number | null;
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
  
  // Color columns (may not exist in all files)
  const colorTop1Column = table.getChild('original_color_top1');
  const colorTop1CountColumn = table.getChild('original_color_top1_count');
  const colorTop2Column = table.getChild('original_color_top2');
  const colorTop2CountColumn = table.getChild('original_color_top2_count');
  const colorTop3Column = table.getChild('original_color_top3');
  const colorTop3CountColumn = table.getChild('original_color_top3_count');
  const colorTop4Column = table.getChild('original_color_top4');
  const colorTop4CountColumn = table.getChild('original_color_top4_count');
  const colorTop5Column = table.getChild('original_color_top5');
  const colorTop5CountColumn = table.getChild('original_color_top5_count');
  
  // Gene columns (may not exist in all files)
  const creatureSizeMeanColumn = table.getChild('creature_size_mean');
  const creatureSizeAvgColumn = table.getChild('creature_size_avg');
  const canKillTrueFractionColumn = table.getChild('can_kill_true_fraction');
  const canKillAverageColumn = table.getChild('can_kill_average');
  const canMoveTrueFractionColumn = table.getChild('can_move_true_fraction');
  const canMoveAverageColumn = table.getChild('can_move_average');
  
  // Health and age columns (may not exist in all files)
  const healthMeanColumn = table.getChild('health_mean');
  const healthAvgColumn = table.getChild('health_avg');
  const ageMeanColumn = table.getChild('age_mean');
  const ageAvgColumn = table.getChild('age_avg');
  
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
        original_color_top1: colorTop1Column?.get(i) ?? null,
        original_color_top1_count: colorTop1CountColumn?.get(i) ? Number(colorTop1CountColumn.get(i)) : null,
        original_color_top2: colorTop2Column?.get(i) ?? null,
        original_color_top2_count: colorTop2CountColumn?.get(i) ? Number(colorTop2CountColumn.get(i)) : null,
        original_color_top3: colorTop3Column?.get(i) ?? null,
        original_color_top3_count: colorTop3CountColumn?.get(i) ? Number(colorTop3CountColumn.get(i)) : null,
        original_color_top4: colorTop4Column?.get(i) ?? null,
        original_color_top4_count: colorTop4CountColumn?.get(i) ? Number(colorTop4CountColumn.get(i)) : null,
        original_color_top5: colorTop5Column?.get(i) ?? null,
        original_color_top5_count: colorTop5CountColumn?.get(i) ? Number(colorTop5CountColumn.get(i)) : null,
        creature_size_mean: creatureSizeMeanColumn?.get(i) !== null && creatureSizeMeanColumn?.get(i) !== undefined ? Number(creatureSizeMeanColumn.get(i)) : null,
        creature_size_avg: creatureSizeAvgColumn?.get(i) !== null && creatureSizeAvgColumn?.get(i) !== undefined ? Number(creatureSizeAvgColumn.get(i)) : null,
        can_kill_true_fraction: canKillTrueFractionColumn?.get(i) !== null && canKillTrueFractionColumn?.get(i) !== undefined ? Number(canKillTrueFractionColumn.get(i)) : null,
        can_kill_average: canKillAverageColumn?.get(i) !== null && canKillAverageColumn?.get(i) !== undefined ? Number(canKillAverageColumn.get(i)) : null,
        can_move_true_fraction: canMoveTrueFractionColumn?.get(i) !== null && canMoveTrueFractionColumn?.get(i) !== undefined ? Number(canMoveTrueFractionColumn.get(i)) : null,
        can_move_average: canMoveAverageColumn?.get(i) !== null && canMoveAverageColumn?.get(i) !== undefined ? Number(canMoveAverageColumn.get(i)) : null,
        health_mean: healthMeanColumn?.get(i) !== null && healthMeanColumn?.get(i) !== undefined ? Number(healthMeanColumn.get(i)) : null,
        health_avg: healthAvgColumn?.get(i) !== null && healthAvgColumn?.get(i) !== undefined ? Number(healthAvgColumn.get(i)) : null,
        age_mean: ageMeanColumn?.get(i) !== null && ageMeanColumn?.get(i) !== undefined ? Number(ageMeanColumn.get(i)) : null,
        age_avg: ageAvgColumn?.get(i) !== null && ageAvgColumn?.get(i) !== undefined ? Number(ageAvgColumn.get(i)) : null,
      });
    }
  }
  
  return rows;
}

export interface EventData {
  tick: number;
  event_type: string;
  event_description: string | null;
}

export async function loadEventsArrow(url: string): Promise<EventData[]> {
  try {
    const response = await fetch(url);
    if (!response.ok) {
      // Events file might not exist, return empty array
      return [];
    }
    
    const arrayBuffer = await response.arrayBuffer();
    const table = tableFromIPC(arrayBuffer);
    
    const tickColumn = table.getChild('tick');
    const eventTypeColumn = table.getChild('event_type');
    const eventDescColumn = table.getChild('event_description');
    
    if (!tickColumn) {
      return [];
    }
    
    const events: EventData[] = [];
    for (let i = 0; i < table.numRows; i++) {
      const tick = tickColumn.get(i);
      if (tick !== null && tick !== undefined) {
        const eventType = eventTypeColumn?.get(i);
        const eventDesc = eventDescColumn?.get(i);
        
        events.push({
          tick: Number(tick),
          event_type: eventType ? String(eventType) : 'Unknown',
          event_description: eventDesc ? String(eventDesc) : null,
        });
      }
    }
    
    return events;
  } catch (err) {
    // If events file doesn't exist or can't be loaded, return empty array
    return [];
  }
}

export interface ImageData {
  tick: number;
  file_name: string;
}

export async function loadImagesArrow(url: string): Promise<ImageData[]> {
  try {
    const response = await fetch(url);
    if (!response.ok) {
      // Images file might not exist, return empty array
      return [];
    }
    
    const arrayBuffer = await response.arrayBuffer();
    const table = tableFromIPC(arrayBuffer);
    
    const tickColumn = table.getChild('tick');
    const fileNameColumn = table.getChild('file_name');
    
    if (!tickColumn || !fileNameColumn) {
      return [];
    }
    
    const images: ImageData[] = [];
    for (let i = 0; i < table.numRows; i++) {
      const tick = tickColumn.get(i);
      const fileName = fileNameColumn.get(i);
      
      if (tick !== null && tick !== undefined && fileName !== null && fileName !== undefined) {
        images.push({
          tick: Number(tick),
          file_name: String(fileName),
        });
      }
    }
    
    return images;
  } catch (err) {
    // If images file doesn't exist or can't be loaded, return empty array
    return [];
  }
}

