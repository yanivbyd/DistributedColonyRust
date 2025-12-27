import { useEffect, useState } from 'react';
import ReactECharts from 'echarts-for-react';
import { loadStatsArrow, loadEventsArrow, EventData } from '../utils/arrowLoader';
import { transformStatsData, ChartDataPoint } from '../utils/dataTransform';

interface CreatureCoverageChartProps {
  colonyId: string | null;
}

export function CreatureCoverageChart({ colonyId }: CreatureCoverageChartProps) {
  const [data, setData] = useState<ChartDataPoint[]>([]);
  const [events, setEvents] = useState<EventData[]>([]);
  const [selectedEvent, setSelectedEvent] = useState<EventData | null>(null);
  const [showEvents, setShowEvents] = useState<boolean>(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!colonyId) {
      setData([]);
      setEvents([]);
      setSelectedEvent(null);
      setError(null);
      return;
    }

    async function loadData() {
      try {
        setLoading(true);
        setError(null);
        const statsUrl = `/bi/${colonyId}/stats.arrow`;
        const eventsUrl = `/bi/${colonyId}/events.arrow`;
        
        const [rows, eventData] = await Promise.all([
          loadStatsArrow(statsUrl),
          loadEventsArrow(eventsUrl),
        ]);
        
        const transformed = transformStatsData(rows);
        setData(transformed);
        setEvents(eventData);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load data');
        setData([]);
        setEvents([]);
      } finally {
        setLoading(false);
      }
    }

    loadData();
  }, [colonyId]);

  if (!colonyId) {
    return (
      <div className="alert alert-info text-light bg-secondary" role="alert">
        Please select a colony to view the chart.
      </div>
    );
  }

  if (loading) {
    return (
      <div className="text-center py-5">
        <div className="spinner-border text-light" role="status">
          <span className="visually-hidden">Loading...</span>
        </div>
        <div className="text-light mt-2">Loading chart data...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="alert alert-danger" role="alert">
        Error: {error}
      </div>
    );
  }

  if (data.length === 0) {
    return (
      <div className="alert alert-warning text-light bg-secondary" role="alert">
        No data available for this colony.
      </div>
    );
  }

  const ticks = data.map((d) => d.tick);
  const creaturePcts = data.map((d) => d.creaturePct);
  const emptyCellsPcts = data.map((d) => d.emptyCellsPct);

  // Filter events to only those within the chart's tick range
  const minTick = Math.min(...ticks);
  const maxTick = Math.max(...ticks);
  const validEvents = events.filter((event) => event.tick >= minTick && event.tick <= maxTick);

  // Get color for event type
  const getEventColor = (eventType: string): string => {
    const type = eventType.toLowerCase();
    if (type.includes('more food')) {
      return '#28a745'; // Dark green for dark mode
    } else if (type.includes('less food')) {
      return '#dc3545'; // Darker red for dark mode
    } else {
      return '#ffc107'; // Darker yellow/amber for dark mode
    }
  };

  // Create markLines for events with color coding
  // Find the closest tick for each event (since events might not match exactly)
  const eventMarkLines = validEvents.map((event) => {
    // Find the closest tick in the stats data
    let closestTick = ticks[0];
    let minDiff = Math.abs(ticks[0] - event.tick);
    
    for (const tick of ticks) {
      const diff = Math.abs(tick - event.tick);
      if (diff < minDiff) {
        minDiff = diff;
        closestTick = tick;
      }
    }
    
    const eventColor = getEventColor(event.event_type);
    
    // Use the tick value as string for category axis
    return {
      xAxis: String(closestTick),
      lineStyle: {
        color: eventColor,
        width: 3,
        type: 'dashed' as const,
        opacity: 0.9,
      },
      label: {
        show: true,
        position: 'end',
        formatter: event.event_type,
        color: eventColor,
        fontSize: 10,
      },
      // Store event data for click handler
      eventData: event,
    };
  });

  const option = {
    backgroundColor: 'transparent',
    textStyle: {
      color: '#e0e0e0',
    },
    title: {
      text: `Creature Coverage`,
      left: 'center',
      textStyle: {
        color: '#ffffff',
      },
    },
    tooltip: {
      trigger: 'axis',
      axisPointer: {
        type: 'cross',
      },
      backgroundColor: '#2d2d2d',
      borderColor: '#444',
      textStyle: {
        color: '#e0e0e0',
      },
      formatter: (params: any) => {
        if (!Array.isArray(params) || params.length === 0) {
          return '';
        }
        const tick = params[0].axisValue;
        const creaturePct = params[0].value as number;
        const emptyPct = params[1]?.value as number;
        return `
          <div>
            <strong>Tick: ${tick}</strong><br/>
            Creature Coverage: ${creaturePct.toFixed(2)}%<br/>
            Empty Cells: ${emptyPct.toFixed(2)}%
          </div>
        `;
      },
    },
    legend: {
      data: ['Creature Coverage', 'Empty Cells'],
      top: 30,
      textStyle: {
        color: '#e0e0e0',
      },
    },
    grid: {
      left: '3%',
      right: '4%',
      bottom: '3%',
      top: '15%',
      containLabel: true,
    },
    xAxis: {
      type: 'category',
      boundaryGap: false,
      data: ticks,
      name: 'Tick',
      nameTextStyle: {
        color: '#e0e0e0',
      },
      axisLine: {
        lineStyle: {
          color: '#666',
        },
      },
      axisLabel: {
        color: '#b0b0b0',
      },
    },
    yAxis: {
      type: 'value',
      name: 'Percentage (%)',
      min: 0,
      max: 100,
      nameTextStyle: {
        color: '#e0e0e0',
      },
      axisLine: {
        lineStyle: {
          color: '#666',
        },
      },
      axisLabel: {
        formatter: '{value}%',
        color: '#b0b0b0',
      },
      splitLine: {
        lineStyle: {
          color: '#333',
        },
      },
    },
    series: [
      {
        name: 'Creature Coverage',
        type: 'line',
        stack: 'Total',
        areaStyle: {
          color: '#4a90e2',
        },
        lineStyle: {
          color: '#4a90e2',
        },
        itemStyle: {
          color: '#4a90e2',
        },
        data: creaturePcts,
        smooth: true,
        markLine: showEvents && eventMarkLines.length > 0 ? {
          data: eventMarkLines.map(({ eventData, ...line }) => line),
          silent: false,
          symbol: ['none', 'none'],
          animation: false,
        } : undefined,
      },
      {
        name: 'Empty Cells',
        type: 'line',
        stack: 'Total',
        areaStyle: {
          color: '#f5f5f5',
        },
        lineStyle: {
          color: '#f5f5f5',
        },
        itemStyle: {
          color: '#f5f5f5',
        },
        data: emptyCellsPcts,
        smooth: true,
      },
      // Invisible series for event clicks (only if events are shown)
      ...(showEvents && validEvents.length > 0 ? [{
        name: 'Events',
        type: 'scatter' as const,
        data: validEvents.map((event) => {
          // Find closest tick index
          let closestIndex = 0;
          let minDiff = Math.abs(ticks[0] - event.tick);
          for (let i = 0; i < ticks.length; i++) {
            const diff = Math.abs(ticks[i] - event.tick);
            if (diff < minDiff) {
              minDiff = diff;
              closestIndex = i;
            }
          }
          return [closestIndex, 50]; // Position at middle of chart
        }),
        symbolSize: 20,
        itemStyle: {
          color: 'transparent',
        },
        label: {
          show: false,
        },
        tooltip: {
          show: false,
        },
        // Store event data for click handler
        eventData: validEvents,
      }] : []),
    ],
  };

  const onChartClick = (params: any) => {
    // Check if click is on the Events series (invisible scatter points)
    if (params.seriesName === 'Events' && params.dataIndex !== undefined) {
      const eventIndex = params.dataIndex;
      if (eventIndex >= 0 && eventIndex < validEvents.length) {
        setSelectedEvent(validEvents[eventIndex]);
      }
    }
    // Also check if click is near a markLine
    else if (params.componentType === 'markLine') {
      const clickedValue = params.value;
      if (clickedValue !== undefined) {
        const clickedTick = typeof clickedValue === 'number' ? clickedValue : Number(clickedValue);
        
        // Find the event with the closest tick
        let closestEvent: EventData | null = null;
        let minDiff = Infinity;
        
        for (const event of validEvents) {
          const diff = Math.abs(event.tick - clickedTick);
          if (diff < minDiff && diff < 200) {
            minDiff = diff;
            closestEvent = event;
          }
        }
        
        if (closestEvent) {
          setSelectedEvent(closestEvent);
        }
      }
    }
  };

  return (
    <>
      <div className="mb-3">
        <div className="form-check">
          <input
            className="form-check-input"
            type="checkbox"
            id="showEventsCheckbox"
            checked={showEvents}
            onChange={(e) => setShowEvents(e.target.checked)}
          />
          <label className="form-check-label text-light" htmlFor="showEventsCheckbox">
            Show Events
          </label>
        </div>
      </div>
      {selectedEvent && (
        <div className="modal show d-block" tabIndex={-1} style={{ backgroundColor: 'rgba(0,0,0,0.5)' }}>
          <div className="modal-dialog modal-dialog-centered">
            <div className="modal-content bg-dark text-light border-secondary">
              <div className="modal-header border-secondary">
                <h5 className="modal-title">Event Details</h5>
                <button
                  type="button"
                  className="btn-close btn-close-white"
                  onClick={() => setSelectedEvent(null)}
                ></button>
              </div>
              <div className="modal-body">
                <p><strong>Tick:</strong> {selectedEvent.tick}</p>
                <p><strong>Event Type:</strong> {selectedEvent.event_type}</p>
                {selectedEvent.event_description && (
                  <p><strong>Description:</strong> {selectedEvent.event_description}</p>
                )}
              </div>
              <div className="modal-footer border-secondary">
                <button
                  type="button"
                  className="btn btn-secondary"
                  onClick={() => setSelectedEvent(null)}
                >
                  Close
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
      <div
        style={{
          width: '100%',
          height: '600px',
          border: '2px solid #444',
          borderRadius: '8px',
          padding: '10px',
          backgroundColor: '#1a1a1a',
          boxShadow: '0 4px 6px rgba(0, 0, 0, 0.3)',
        }}
      >
        <ReactECharts
          option={option}
          style={{ height: '100%', width: '100%' }}
          onEvents={{
            click: onChartClick,
          }}
        />
      </div>
    </>
  );
}

