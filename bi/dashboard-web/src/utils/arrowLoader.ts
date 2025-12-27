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

