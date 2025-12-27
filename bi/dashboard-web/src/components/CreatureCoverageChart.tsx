import { useEffect, useState } from 'react';
import ReactECharts from 'echarts-for-react';
import { loadStatsArrow, loadEventsArrow, loadImagesArrow, EventData, ImageData, StatsRow } from '../utils/arrowLoader';
import { transformStatsData, transformColorCountData, ChartDataPoint, ColorCountDataPoint } from '../utils/dataTransform';

interface CreatureCoverageChartProps {
  colonyId: string | null;
}

export function CreatureCoverageChart({ colonyId }: CreatureCoverageChartProps) {
  const [data, setData] = useState<ChartDataPoint[]>([]);
  const [rows, setRows] = useState<StatsRow[]>([]);
  const [colorData, setColorData] = useState<ColorCountDataPoint[]>([]);
  const [colorMap, setColorMap] = useState<Map<string, string>>(new Map());
  const [events, setEvents] = useState<EventData[]>([]);
  const [images, setImages] = useState<ImageData[]>([]);
  const [selectedEvent, setSelectedEvent] = useState<EventData | null>(null);
  const [selectedImage, setSelectedImage] = useState<ImageData | null>(null);
  const [showEvents, setShowEvents] = useState<boolean>(false);
  const [showImages, setShowImages] = useState<boolean>(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [imageModalPosition, setImageModalPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });

  useEffect(() => {
    if (!colonyId) {
      setData([]);
      setRows([]);
      setColorData([]);
      setColorMap(new Map());
      setEvents([]);
      setImages([]);
      setSelectedEvent(null);
      setSelectedImage(null);
      setError(null);
      return;
    }

    async function loadData() {
      try {
        setLoading(true);
        setError(null);
        const statsUrl = `/bi/${colonyId}/stats.arrow`;
        const eventsUrl = `/bi/${colonyId}/events.arrow`;
        const imagesUrl = `/bi/${colonyId}/images.arrow`;
        
        const [rowsData, eventData, imageData] = await Promise.all([
          loadStatsArrow(statsUrl),
          loadEventsArrow(eventsUrl),
          loadImagesArrow(imagesUrl),
        ]);
        
        const transformed = transformStatsData(rowsData);
        const { data: colorCountData, colors } = transformColorCountData(rowsData);
        setData(transformed);
        setRows(rowsData);
        setColorData(colorCountData);
        setColorMap(colors);
        setEvents(eventData);
        setImages(imageData);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load data');
        setData([]);
        setRows([]);
        setColorData([]);
        setColorMap(new Map());
        setEvents([]);
        setImages([]);
      } finally {
        setLoading(false);
      }
    }

    loadData();
  }, [colonyId]);

  // Handle image modal drag
  const handleImageModalMouseDown = (e: React.MouseEvent) => {
    if (selectedImage) {
      setIsDragging(true);
      const modalDialog = (e.currentTarget as HTMLElement).closest('.modal-dialog') as HTMLElement;
      if (modalDialog) {
        const rect = modalDialog.getBoundingClientRect();
        setDragStart({
          x: e.clientX - rect.left,
          y: e.clientY - rect.top,
        });
      }
    }
  };

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (isDragging && selectedImage) {
        const newX = e.clientX - dragStart.x;
        const newY = e.clientY - dragStart.y;
        
        // Keep modal within viewport bounds (600px width, ~500px height)
        const modalWidth = 600;
        const modalHeight = 500;
        const maxX = window.innerWidth - modalWidth;
        const maxY = window.innerHeight - modalHeight;
        
        setImageModalPosition({
          x: Math.max(0, Math.min(newX, maxX)),
          y: Math.max(0, Math.min(newY, maxY)),
        });
      }
    };

    const handleMouseUp = () => {
      setIsDragging(false);
    };

    if (isDragging) {
      document.addEventListener('mousemove', handleMouseMove);
      document.addEventListener('mouseup', handleMouseUp);
      return () => {
        document.removeEventListener('mousemove', handleMouseMove);
        document.removeEventListener('mouseup', handleMouseUp);
      };
    }
  }, [isDragging, dragStart, selectedImage]);

  // Reset position when image modal opens
  useEffect(() => {
    if (selectedImage) {
      // Center the modal initially (600px width, ~500px height)
      const centerX = window.innerWidth / 2 - 300;
      const centerY = window.innerHeight / 2 - 250;
      setImageModalPosition({ x: centerX, y: centerY });
    }
  }, [selectedImage]);

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

  // Filter events and images to only those within the chart's tick range
  const minTick = Math.min(...ticks);
  const maxTick = Math.max(...ticks);
  const validEvents = events.filter((event) => event.tick >= minTick && event.tick <= maxTick);
  const validImages = images.filter((image) => image.tick >= minTick && image.tick <= maxTick);

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

  // Create markLines for images (purple vertical lines)
  const imageMarkLines = validImages.map((image) => {
    // Find the closest tick in the stats data
    let closestTick = ticks[0];
    let minDiff = Math.abs(ticks[0] - image.tick);
    
    for (const tick of ticks) {
      const diff = Math.abs(tick - image.tick);
      if (diff < minDiff) {
        minDiff = diff;
        closestTick = tick;
      }
    }
    
    // Use dark purple color for images
    const imageColor = '#6a1b9a';
    
    return {
      xAxis: String(closestTick),
      lineStyle: {
        color: imageColor,
        width: 2,
        type: 'solid' as const,
        opacity: 0.8,
      },
      label: {
        show: true,
        position: 'start',
        formatter: '📷',
        color: imageColor,
        fontSize: 12,
      },
      // Store image data for click handler
      imageData: image,
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
        markLine: (showEvents && eventMarkLines.length > 0) || (showImages && imageMarkLines.length > 0) ? {
          data: [
            ...(showEvents ? eventMarkLines.map(({ eventData, ...line }) => line) : []),
            ...(showImages ? imageMarkLines.map(({ imageData, ...line }) => line) : []),
          ],
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
      // Invisible series for image clicks (only if images are shown)
      ...(showImages && validImages.length > 0 ? [{
        name: 'Images',
        type: 'scatter' as const,
        data: validImages.map((image) => {
          // Find closest tick index
          let closestIndex = 0;
          let minDiff = Math.abs(ticks[0] - image.tick);
          for (let i = 0; i < ticks.length; i++) {
            const diff = Math.abs(ticks[i] - image.tick);
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
        // Store image data for click handler
        imageData: validImages,
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
    // Check if click is on the Images series (invisible scatter points)
    else if (params.seriesName === 'Images' && params.dataIndex !== undefined) {
      const imageIndex = params.dataIndex;
      if (imageIndex >= 0 && imageIndex < validImages.length) {
        setSelectedImage(validImages[imageIndex]);
      }
    }
    // Also check if click is near a markLine
    else if (params.componentType === 'markLine') {
      const clickedValue = params.value;
      if (clickedValue !== undefined) {
        const clickedTick = typeof clickedValue === 'number' ? clickedValue : Number(clickedValue);
        
        // First check if it's an image
        let closestImage: ImageData | null = null;
        let minImageDiff = Infinity;
        
        for (const image of validImages) {
          const diff = Math.abs(image.tick - clickedTick);
          if (diff < minImageDiff && diff < 200) {
            minImageDiff = diff;
            closestImage = image;
          }
        }
        
        if (closestImage) {
          setSelectedImage(closestImage);
          return;
        }
        
        // Then check if it's an event
        let closestEvent: EventData | null = null;
        let minEventDiff = Infinity;
        
        for (const event of validEvents) {
          const diff = Math.abs(event.tick - clickedTick);
          if (diff < minEventDiff && diff < 200) {
            minEventDiff = diff;
            closestEvent = event;
          }
        }
        
        if (closestEvent) {
          setSelectedEvent(closestEvent);
        }
      }
    }
  };

  // Helper function to create mark lines for events and images
  const createMarkLines = (ticks: number[]) => {
    const lines: any[] = [];
    
    if (showEvents) {
      validEvents.forEach((event) => {
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
        
        lines.push({
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
        });
      });
    }
    
    if (showImages) {
      validImages.forEach((image) => {
        let closestTick = ticks[0];
        let minDiff = Math.abs(ticks[0] - image.tick);
        
        for (const tick of ticks) {
          const diff = Math.abs(tick - image.tick);
          if (diff < minDiff) {
            minDiff = diff;
            closestTick = tick;
          }
        }
        
        const imageColor = '#6a1b9a';
        
        lines.push({
          xAxis: String(closestTick),
          lineStyle: {
            color: imageColor,
            width: 2,
            type: 'solid' as const,
            opacity: 0.8,
          },
          label: {
            show: true,
            position: 'start',
            formatter: '📷',
            color: imageColor,
            fontSize: 12,
          },
        });
      });
    }
    
    return lines;
  };

  // Create color count chart option
  const colorTicks = colorData.map((d) => d.tick);
  const colorNames = Array.from(colorMap.keys());
  
  const colorChartOption = {
    backgroundColor: 'transparent',
    textStyle: {
      color: '#e0e0e0',
    },
    title: {
      text: `Creature Count by Color`,
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
    },
    grid: {
      left: '3%',
      right: '4%',
      bottom: '3%',
      top: '10%',
      containLabel: true,
    },
    xAxis: {
      type: 'category',
      boundaryGap: false,
      data: colorTicks,
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
      name: 'Creature Count',
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
      splitLine: {
        lineStyle: {
          color: '#333',
        },
      },
    },
    series: [
      ...colorNames.map((colorName, index) => ({
        name: colorName,
        type: 'line' as const,
        data: colorData.map((point) => point[colorName] || 0),
        smooth: true,
        areaStyle: {
          color: colorMap.get(colorName) || '#808080',
          opacity: 0.3,
        },
        lineStyle: {
          color: colorMap.get(colorName) || '#808080',
          width: 2,
        },
        itemStyle: {
          color: colorMap.get(colorName) || '#808080',
        },
        // Add mark lines only to the first series
        ...(index === 0 && createMarkLines(colorTicks).length > 0 ? {
          markLine: {
            data: createMarkLines(colorTicks),
            silent: false,
            symbol: ['none', 'none'],
            animation: false,
          }
        } : {}),
      })),
      // Invisible series for event clicks (only if events are shown)
      ...(showEvents && validEvents.length > 0 ? [{
        name: 'Events',
        type: 'scatter' as const,
        data: validEvents.map((event) => {
          let closestIndex = 0;
          let minDiff = Math.abs(colorTicks[0] - event.tick);
          for (let i = 0; i < colorTicks.length; i++) {
            const diff = Math.abs(colorTicks[i] - event.tick);
            if (diff < minDiff) {
              minDiff = diff;
              closestIndex = i;
            }
          }
          const midValue = colorData[closestIndex] ? 
            Object.values(colorData[closestIndex]).reduce((sum: number, val) => 
              typeof val === 'number' ? sum + val : sum, 0) / 2 : 0;
          return [closestIndex, midValue];
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
      }] : []),
      // Invisible series for image clicks (only if images are shown)
      ...(showImages && validImages.length > 0 ? [{
        name: 'Images',
        type: 'scatter' as const,
        data: validImages.map((image) => {
          let closestIndex = 0;
          let minDiff = Math.abs(colorTicks[0] - image.tick);
          for (let i = 0; i < colorTicks.length; i++) {
            const diff = Math.abs(colorTicks[i] - image.tick);
            if (diff < minDiff) {
              minDiff = diff;
              closestIndex = i;
            }
          }
          const midValue = colorData[closestIndex] ? 
            Object.values(colorData[closestIndex]).reduce((sum: number, val) => 
              typeof val === 'number' ? sum + val : sum, 0) / 2 : 0;
          return [closestIndex, midValue];
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
      }] : []),
    ],
  };

  // Helper function to create gene chart option
  const createGeneChartOption = (
    title: string,
    yAxisName: string,
    data: number[],
    ticks: number[],
    color: string
  ) => {
    const markLines = createMarkLines(ticks);
    return {
      backgroundColor: 'transparent',
      textStyle: {
        color: '#e0e0e0',
      },
      title: {
        text: title,
        left: 'center',
        textStyle: {
          color: '#ffffff',
          fontSize: 14,
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
      },
      grid: {
        left: '10%',
        right: '10%',
        bottom: '15%',
        top: '20%',
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
          fontSize: 10,
        },
      },
      yAxis: {
        type: 'value',
        name: yAxisName,
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
          fontSize: 10,
        },
        splitLine: {
          lineStyle: {
            color: '#333',
          },
        },
      },
      series: [
        {
          name: title,
          type: 'line' as const,
          data: data,
          smooth: true,
          areaStyle: {
            color: color,
            opacity: 0.3,
          },
          lineStyle: {
            color: color,
            width: 2,
          },
          itemStyle: {
            color: color,
          },
          markLine: markLines.length > 0 ? {
            data: markLines,
            silent: false,
            symbol: ['none', 'none'],
            animation: false,
          } : undefined,
        },
        // Invisible series for event clicks
        ...(showEvents && validEvents.length > 0 ? [{
          name: 'Events',
          type: 'scatter' as const,
          data: validEvents.map((event) => {
            let closestIndex = 0;
            let minDiff = Math.abs(ticks[0] - event.tick);
            for (let i = 0; i < ticks.length; i++) {
              const diff = Math.abs(ticks[i] - event.tick);
              if (diff < minDiff) {
                minDiff = diff;
                closestIndex = i;
              }
            }
            const midValue = data[closestIndex] || 0;
            return [closestIndex, midValue];
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
        }] : []),
        // Invisible series for image clicks
        ...(showImages && validImages.length > 0 ? [{
          name: 'Images',
          type: 'scatter' as const,
          data: validImages.map((image) => {
            let closestIndex = 0;
            let minDiff = Math.abs(ticks[0] - image.tick);
            for (let i = 0; i < ticks.length; i++) {
              const diff = Math.abs(ticks[i] - image.tick);
              if (diff < minDiff) {
                minDiff = diff;
                closestIndex = i;
              }
            }
            const midValue = data[closestIndex] || 0;
            return [closestIndex, midValue];
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
        }] : []),
      ],
    };
  };

  // Helper function to create stacked boolean chart option
  const createStackedBooleanChartOption = (
    title: string,
    trueData: number[],
    falseData: number[],
    ticks: number[]
  ) => {
    const markLines = createMarkLines(ticks);
    return {
      backgroundColor: 'transparent',
      textStyle: {
        color: '#e0e0e0',
      },
      title: {
        text: title,
        left: 'center',
        textStyle: {
          color: '#ffffff',
          fontSize: 14,
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
      },
      legend: {
        data: ['True', 'False'],
        top: 30,
        textStyle: {
          color: '#e0e0e0',
          fontSize: 11,
        },
      },
      grid: {
        left: '10%',
        right: '10%',
        bottom: '15%',
        top: '25%',
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
          fontSize: 10,
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
          color: '#b0b0b0',
          fontSize: 10,
          formatter: '{value}%',
        },
        splitLine: {
          lineStyle: {
            color: '#333',
          },
        },
      },
      series: [
        {
          name: 'True',
          type: 'line' as const,
          stack: 'Total',
          data: trueData,
          smooth: true,
          areaStyle: {
            color: '#dc3545', // Red for true
            opacity: 0.6,
          },
          lineStyle: {
            color: '#dc3545',
            width: 2,
          },
          itemStyle: {
            color: '#dc3545',
          },
          markLine: markLines.length > 0 ? {
            data: markLines,
            silent: false,
            symbol: ['none', 'none'],
            animation: false,
          } : undefined,
        },
        {
          name: 'False',
          type: 'line' as const,
          stack: 'Total',
          data: falseData,
          smooth: true,
          areaStyle: {
            color: '#28a745', // Green for false
            opacity: 0.6,
          },
          lineStyle: {
            color: '#28a745',
            width: 2,
          },
          itemStyle: {
            color: '#28a745',
          },
        },
        // Invisible series for event clicks
        ...(showEvents && validEvents.length > 0 ? [{
          name: 'Events',
          type: 'scatter' as const,
          data: validEvents.map((event) => {
            let closestIndex = 0;
            let minDiff = Math.abs(ticks[0] - event.tick);
            for (let i = 0; i < ticks.length; i++) {
              const diff = Math.abs(ticks[i] - event.tick);
              if (diff < minDiff) {
                minDiff = diff;
                closestIndex = i;
              }
            }
            return [closestIndex, 50]; // Middle of chart
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
        }] : []),
        // Invisible series for image clicks
        ...(showImages && validImages.length > 0 ? [{
          name: 'Images',
          type: 'scatter' as const,
          data: validImages.map((image) => {
            let closestIndex = 0;
            let minDiff = Math.abs(ticks[0] - image.tick);
            for (let i = 0; i < ticks.length; i++) {
              const diff = Math.abs(ticks[i] - image.tick);
              if (diff < minDiff) {
                minDiff = diff;
                closestIndex = i;
              }
            }
            return [closestIndex, 50]; // Middle of chart
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
        }] : []),
      ],
    };
  };

  // Prepare gene chart data
  const geneTicks = data.map((d) => d.tick);

  // Create gene chart options
  const creatureSizeChartOption = createGeneChartOption(
    'Creature Size',
    'Mean Size',
    data.map((d) => {
      const row = rows.find((r) => r.tick === d.tick);
      return row?.creature_size_mean ?? row?.creature_size_avg ?? 0;
    }),
    geneTicks,
    '#4a90e2'
  );

  // Prepare stacked boolean data for Can Kill
  const canKillTrueData = data.map((d) => {
    const row = rows.find((r) => r.tick === d.tick);
    return (row?.can_kill_true_fraction ?? row?.can_kill_average ?? 0) * 100; // Convert to percentage
  });
  const canKillFalseData = data.map((d) => {
    const row = rows.find((r) => r.tick === d.tick);
    const trueFraction = row?.can_kill_true_fraction ?? row?.can_kill_average ?? 0;
    return (1 - trueFraction) * 100; // False = 100% - true%
  });
  const canKillChartOption = createStackedBooleanChartOption(
    'Can Kill',
    canKillTrueData,
    canKillFalseData,
    geneTicks
  );

  // Prepare stacked boolean data for Can Move
  const canMoveTrueData = data.map((d) => {
    const row = rows.find((r) => r.tick === d.tick);
    return (row?.can_move_true_fraction ?? row?.can_move_average ?? 0) * 100; // Convert to percentage
  });
  const canMoveFalseData = data.map((d) => {
    const row = rows.find((r) => r.tick === d.tick);
    const trueFraction = row?.can_move_true_fraction ?? row?.can_move_average ?? 0;
    return (1 - trueFraction) * 100; // False = 100% - true%
  });
  const canMoveChartOption = createStackedBooleanChartOption(
    'Can Move',
    canMoveTrueData,
    canMoveFalseData,
    geneTicks
  );

  // Create age and health chart options
  const ageChartOption = createGeneChartOption(
    'Age',
    'Mean Age',
    data.map((d) => {
      const row = rows.find((r) => r.tick === d.tick);
      return row?.age_mean ?? row?.age_avg ?? 0;
    }),
    geneTicks,
    '#ffc107'
  );

  const healthChartOption = createGeneChartOption(
    'Health',
    'Mean Health',
    data.map((d) => {
      const row = rows.find((r) => r.tick === d.tick);
      return row?.health_mean ?? row?.health_avg ?? 0;
    }),
    geneTicks,
    '#17a2b8'
  );

  return (
    <div style={{ display: 'flex', gap: '20px', width: '100%' }}>
      {/* Left Sidebar */}
      <div
        style={{
          width: '200px',
          minWidth: '200px',
          backgroundColor: '#1a1a1a',
          border: '2px solid #444',
          borderRadius: '8px',
          padding: '15px',
          height: 'fit-content',
          position: 'sticky',
          top: '20px',
          alignSelf: 'flex-start',
        }}
      >
        <h6 className="text-light mb-3" style={{ fontSize: '14px', fontWeight: 'bold' }}>Controls</h6>
        <div className="form-check mb-3">
          <input
            className="form-check-input"
            type="checkbox"
            id="showEventsCheckbox"
            checked={showEvents}
            onChange={(e) => setShowEvents(e.target.checked)}
          />
          <label className="form-check-label text-light" htmlFor="showEventsCheckbox" style={{ fontSize: '13px' }}>
            Show Events ({validEvents.length})
          </label>
        </div>
        <div className="form-check">
          <input
            className="form-check-input"
            type="checkbox"
            id="showImagesCheckbox"
            checked={showImages}
            onChange={(e) => setShowImages(e.target.checked)}
          />
          <label className="form-check-label text-light" htmlFor="showImagesCheckbox" style={{ fontSize: '13px' }}>
            Show Images ({validImages.length})
          </label>
        </div>
      </div>

      {/* Main Content Area */}
      <div style={{ flex: 1, minWidth: 0 }}>
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
      {selectedImage && (
        <div className="modal show d-block" tabIndex={-1} style={{ backgroundColor: 'rgba(0,0,0,0.5)' }} onClick={() => setSelectedImage(null)}>
          <div 
            className="modal-dialog" 
            onClick={(e) => e.stopPropagation()}
            style={{
              position: 'absolute',
              left: `${imageModalPosition.x}px`,
              top: `${imageModalPosition.y}px`,
              margin: 0,
              transform: 'none',
              maxWidth: '600px',
              width: '600px',
            }}
          >
            <div className="modal-content bg-dark text-light border-secondary">
              <div 
                className="modal-header border-secondary"
                onMouseDown={handleImageModalMouseDown}
                style={{ cursor: isDragging ? 'grabbing' : 'grab', userSelect: 'none', padding: '8px 15px' }}
              >
                <h6 className="modal-title mb-0">Colony Image - Tick {selectedImage.tick}</h6>
                <button
                  type="button"
                  className="btn-close btn-close-white"
                  onClick={() => setSelectedImage(null)}
                ></button>
              </div>
              <div className="modal-body text-center" style={{ padding: '10px', maxHeight: '450px', overflow: 'auto' }}>
                <img
                  src={`/bi/${colonyId}/images/${selectedImage.file_name}`}
                  alt={`Colony at tick ${selectedImage.tick}`}
                  style={{ maxWidth: '100%', maxHeight: '450px', height: 'auto', objectFit: 'contain' }}
                  onError={(e) => {
                    (e.target as HTMLImageElement).src = 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"><text x="50%25" y="50%25" fill="white">Image not found</text></svg>';
                  }}
                />
              </div>
              <div className="modal-footer border-secondary" style={{ padding: '8px 15px' }}>
                <button
                  type="button"
                  className="btn btn-secondary btn-sm"
                  onClick={() => setSelectedImage(null)}
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
          height: '300px',
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
      
      {/* Color Count Chart */}
      {colorData.length > 0 && colorNames.length > 0 && (
        <div
          style={{
            width: '100%',
            height: '300px',
            border: '2px solid #444',
            borderRadius: '8px',
            padding: '10px',
            backgroundColor: '#1a1a1a',
            boxShadow: '0 4px 6px rgba(0, 0, 0, 0.3)',
            marginTop: '20px',
          }}
        >
          <ReactECharts
            option={colorChartOption}
            style={{ height: '100%', width: '100%' }}
            onEvents={{
              click: onChartClick,
            }}
          />
        </div>
      )}

      {/* Genes Section */}
      {rows.length > 0 && (
        <>
          <h5 className="text-light mt-4 mb-3" style={{ fontSize: '18px', fontWeight: 'bold' }}>Genes</h5>
          <div style={{ display: 'flex', gap: '15px', width: '100%', marginBottom: '20px' }}>
            {/* Creature Size Chart */}
            <div
              style={{
                flex: 1,
                height: '200px',
                border: '2px solid #444',
                borderRadius: '8px',
                padding: '10px',
                backgroundColor: '#1a1a1a',
                boxShadow: '0 4px 6px rgba(0, 0, 0, 0.3)',
              }}
            >
              <ReactECharts
                option={creatureSizeChartOption}
                style={{ height: '100%', width: '100%' }}
                onEvents={{
                  click: onChartClick,
                }}
              />
            </div>

            {/* Can Kill Chart */}
            <div
              style={{
                flex: 1,
                height: '200px',
                border: '2px solid #444',
                borderRadius: '8px',
                padding: '10px',
                backgroundColor: '#1a1a1a',
                boxShadow: '0 4px 6px rgba(0, 0, 0, 0.3)',
              }}
            >
              <ReactECharts
                option={canKillChartOption}
                style={{ height: '100%', width: '100%' }}
                onEvents={{
                  click: onChartClick,
                }}
              />
            </div>

            {/* Can Move Chart */}
            <div
              style={{
                flex: 1,
                height: '200px',
                border: '2px solid #444',
                borderRadius: '8px',
                padding: '10px',
                backgroundColor: '#1a1a1a',
                boxShadow: '0 4px 6px rgba(0, 0, 0, 0.3)',
              }}
            >
              <ReactECharts
                option={canMoveChartOption}
                style={{ height: '100%', width: '100%' }}
                onEvents={{
                  click: onChartClick,
                }}
              />
            </div>
          </div>
        </>
      )}

      {/* Creatures Section */}
      {rows.length > 0 && (
        <>
          <h5 className="text-light mt-4 mb-3" style={{ fontSize: '18px', fontWeight: 'bold' }}>Creatures</h5>
          <div style={{ display: 'flex', gap: '15px', width: '100%', marginBottom: '20px' }}>
            {/* Age Chart */}
            <div
              style={{
                flex: 1,
                height: '200px',
                border: '2px solid #444',
                borderRadius: '8px',
                padding: '10px',
                backgroundColor: '#1a1a1a',
                boxShadow: '0 4px 6px rgba(0, 0, 0, 0.3)',
              }}
            >
              <ReactECharts
                option={ageChartOption}
                style={{ height: '100%', width: '100%' }}
                onEvents={{
                  click: onChartClick,
                }}
              />
            </div>

            {/* Health Chart */}
            <div
              style={{
                flex: 1,
                height: '200px',
                border: '2px solid #444',
                borderRadius: '8px',
                padding: '10px',
                backgroundColor: '#1a1a1a',
                boxShadow: '0 4px 6px rgba(0, 0, 0, 0.3)',
              }}
            >
              <ReactECharts
                option={healthChartOption}
                style={{ height: '100%', width: '100%' }}
                onEvents={{
                  click: onChartClick,
                }}
              />
            </div>
          </div>
        </>
      )}
      </div>
    </div>
  );
}

