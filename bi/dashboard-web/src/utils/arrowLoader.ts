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

