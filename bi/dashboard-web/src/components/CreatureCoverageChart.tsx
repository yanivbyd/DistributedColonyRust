import { useEffect, useState } from 'react';
import ReactECharts from 'echarts-for-react';
import { loadStatsArrow } from '../utils/arrowLoader';
import { transformStatsData, ChartDataPoint } from '../utils/dataTransform';

interface CreatureCoverageChartProps {
  colonyId: string | null;
}

export function CreatureCoverageChart({ colonyId }: CreatureCoverageChartProps) {
  const [data, setData] = useState<ChartDataPoint[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!colonyId) {
      setData([]);
      setError(null);
      return;
    }

    async function loadData() {
      try {
        setLoading(true);
        setError(null);
        const url = `/bi/${colonyId}/stats.arrow`;
        const rows = await loadStatsArrow(url);
        const transformed = transformStatsData(rows);
        setData(transformed);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load data');
        setData([]);
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

  const option = {
    backgroundColor: 'transparent',
    textStyle: {
      color: '#e0e0e0',
    },
    title: {
      text: 'Creature Coverage Over Time',
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
    ],
  };

  return (
    <div className="card bg-dark border-secondary">
      <div className="card-body">
        <div style={{ width: '100%', height: '600px' }}>
          <ReactECharts option={option} style={{ height: '100%', width: '100%' }} />
        </div>
      </div>
    </div>
  );
}

