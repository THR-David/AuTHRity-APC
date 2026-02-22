import React, { useState, useEffect } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer } from 'recharts';

interface TrendsTabProps {
  cvList: string[];
  mvList: string[];
  dvList: string[];
}

interface TrendsDataPoint {
  timestamp: string;
  tag: string;
  field: string;
  value: number;
}

interface VariableConfig {
  key: string;
  enabled: boolean;
  yMin: number | null;
  yMax: number | null;
  autoScale: boolean;
  color: string;
  type: string;
}

export const TrendsTab: React.FC<TrendsTabProps> = ({ cvList, mvList, dvList }) => {
  const [timeRange, setTimeRange] = useState<string>('1h');
  const [loading, setLoading] = useState(false);
  const [trendsData, setTrendsData] = useState<TrendsDataPoint[]>([]);
  const [chartData, setChartData] = useState<any[]>([]);
  const [variables, setVariables] = useState<Record<string, VariableConfig>>({});

  const colors = ['#6366f1', '#8b5cf6', '#ec4899', '#f59e0b', '#10b981', '#3b82f6', '#ef4444', '#f97316'];

  // Build available tag list and initialize variables
  const allTags = [
    ...cvList.map(cv => ({ tag: cv, field: 'PV', type: 'CV' })),
    ...cvList.map(cv => ({ tag: cv, field: 'Target', type: 'CV' })),
    ...mvList.map(mv => ({ tag: mv, field: 'OP', type: 'MV' })),
    ...mvList.map(mv => ({ tag: mv, field: 'SP', type: 'MV' })),
    ...dvList.map(dv => ({ tag: dv, field: 'PV', type: 'DV' })),
  ];

  // Initialize variables config
  useEffect(() => {
    const initialVars: Record<string, VariableConfig> = {};
    allTags.forEach(({ tag, field, type }, idx) => {
      const key = `${tag}:${field}`;
      initialVars[key] = {
        key,
        enabled: false,
        yMin: null,
        yMax: null,
        autoScale: true,
        color: colors[idx % colors.length],
        type
      };
    });
    setVariables(initialVars);
  }, [cvList.length, mvList.length, dvList.length]);

  const toggleVariable = (key: string) => {
    setVariables(prev => ({
      ...prev,
      [key]: { ...prev[key], enabled: !prev[key].enabled }
    }));
  };

  const updateVariableConfig = (key: string, field: 'yMin' | 'yMax' | 'autoScale', value: any) => {
    setVariables(prev => ({
      ...prev,
      [key]: { ...prev[key], [field]: value }
    }));
  };

  const fetchTrendsData = async () => {
    const enabledVars = Object.values(variables).filter(v => v.enabled);
    if (enabledVars.length === 0) return;

    setLoading(true);
    
    const tagsParam = enabledVars.map(v => v.key).join(',');
    const startTime = getStartTime(timeRange);
    
    try {
      const response = await fetch(`/api/trends?tags=${tagsParam}&start=${startTime}`);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      
      const data: TrendsDataPoint[] = await response.json();
      setTrendsData(data);
      
      // Transform data for recharts - use full ISO timestamp as key for better merging
      const grouped: Record<string, any> = {};
      data.forEach(point => {
        const key = point.timestamp;
        if (!grouped[key]) {
          grouped[key] = { 
            timestamp: new Date(point.timestamp).toLocaleTimeString(),
            _sortKey: new Date(point.timestamp).getTime()
          };
        }
        grouped[key][`${point.tag}:${point.field}`] = point.value;
      });
      
      // Sort by actual timestamp and remove sort key
      const chartValues = Object.values(grouped)
        .sort((a, b) => a._sortKey - b._sortKey)
        .map(({ _sortKey, ...rest }) => rest);
      
      // Normalize data based on each variable's yMin/yMax
      const normalizedChartData = chartValues.map(point => {
        const normalized: any = { timestamp: point.timestamp };
        enabledVars.forEach(v => {
          if (point[v.key] !== undefined && point[v.key] !== null) {
            const val = point[v.key];
            // Store original value for tooltip
            normalized[`${v.key}_original`] = val;
            
            // Normalize to 0-100 based on variable's range
            if (v.yMin !== null && v.yMax !== null && v.yMax !== v.yMin) {
              const normalized_val = ((val - v.yMin) / (v.yMax - v.yMin)) * 100;
              normalized[v.key] = normalized_val;
            } else {
              // If no range set, use original value
              normalized[v.key] = val;
            }
          }
        });
        return normalized;
      });
      
      setChartData(normalizedChartData);
      
      // Calculate min/max ONLY for auto-scale variables
      const updatedVars = { ...variables };
      let hasChanges = false;
      enabledVars.forEach(v => {
        if (v.autoScale) {
          // Use original values from chartValues for auto-scale calculation
          const values = chartValues
            .map(row => row[v.key])
            .filter(val => val !== undefined && val !== null) as number[];
          
          if (values.length > 0) {
            const min = Math.min(...values);
            const max = Math.max(...values);
            const padding = (max - min) * 0.1 || 1; // 10% padding or 1 if range is 0
            const newMin = Number((min - padding).toFixed(2));
            const newMax = Number((max + padding).toFixed(2));
            
            // Only update if values actually changed
            if (updatedVars[v.key].yMin !== newMin || updatedVars[v.key].yMax !== newMax) {
              updatedVars[v.key] = {
                ...v,
                yMin: newMin,
                yMax: newMax
              };
              hasChanges = true;
            }
          }
        }
      });
      
      // Only update state if there were actual changes to avoid unnecessary re-renders
      if (hasChanges) {
        setVariables(updatedVars);
      }
    } catch (e) {
      console.error('Failed to fetch trends:', e);
    } finally {
      setLoading(false);
    }
  };

  const getStartTime = (range: string): string => {
    const now = new Date();
    switch (range) {
      case '15m': return new Date(now.getTime() - 15 * 60 * 1000).toISOString();
      case '1h': return new Date(now.getTime() - 60 * 60 * 1000).toISOString();
      case '4h': return new Date(now.getTime() - 4 * 60 * 60 * 1000).toISOString();
      case '12h': return new Date(now.getTime() - 12 * 60 * 60 * 1000).toISOString();
      case '24h': return new Date(now.getTime() - 24 * 60 * 60 * 1000).toISOString();
      default: return new Date(now.getTime() - 60 * 60 * 1000).toISOString();
    }
  };

  useEffect(() => {
    const enabledKeys = Object.values(variables).filter(v => v.enabled).map(v => v.key).sort().join(',');
    if (enabledKeys) {
      fetchTrendsData();
      const interval = setInterval(fetchTrendsData, 30000); // Refresh every 30 seconds
      return () => clearInterval(interval);
    }
  }, [Object.values(variables).filter(v => v.enabled).map(v => v.key).sort().join(','), timeRange]);

  // Calculate Y-axis domain - fixed 0-100 for normalized view
  const getYAxisDomain = (): [number, number] => {
    return [0, 100];
  };
  // Custom tooltip to show original values
  const CustomTooltip = ({ active, payload, label }: any) => {
    if (active && payload && payload.length) {
      return (
        <div style={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px', padding: '8px', fontSize: '11px' }}>
          <p style={{ color: '#cbd5e1', marginBottom: '4px' }}>{label}</p>
          {payload.map((entry: any) => {
            const varKey = entry.dataKey;
            const originalValue = entry.payload[`${varKey}_original`];
            const normalizedValue = entry.value;
            const varConfig = variables[varKey];
            
            return (
              <p key={entry.dataKey} style={{ color: entry.color, margin: '2px 0' }}>
                {entry.name}: {originalValue !== undefined ? originalValue.toFixed(2) : 'N/A'}
                {varConfig && varConfig.yMin !== null && varConfig.yMax !== null && (
                  <span style={{ color: '#94a3b8', fontSize: '10px', marginLeft: '4px' }}>
                    ({normalizedValue.toFixed(1)}%)
                  </span>
                )}
              </p>
            );
          })}
        </div>
      );
    }
    return null;
  };
  return (
    <div className="space-y-4">
      {/* Chart - TOP */}
      <div className="bg-slate-900/50 rounded-xl border border-slate-800 p-4">
        {loading && (
          <div className="h-96 flex items-center justify-center text-slate-500">
            Loading trends data...
          </div>
        )}
        
        {!loading && Object.values(variables).filter(v => v.enabled).length === 0 && (
          <div className="h-96 flex items-center justify-center text-slate-500">
            Select variables below to view historical trends
          </div>
        )}
        
        {!loading && Object.values(variables).filter(v => v.enabled).length > 0 && chartData.length === 0 && (
          <div className="h-96 flex items-center justify-center text-slate-500">
            No data available for selected time range
          </div>
        )}
        
        {!loading && chartData.length > 0 && (
          <ResponsiveContainer width="100%" height={500}>
            <LineChart data={chartData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
              <XAxis 
                dataKey="timestamp" 
                stroke="#94a3b8"
                tick={{ fontSize: 10 }}
              />
              <YAxis 
                stroke="#94a3b8"
                tick={{ fontSize: 10 }}
                domain={getYAxisDomain()}
                label={{ value: '% of Range', angle: -90, position: 'insideLeft', style: { fontSize: 10, fill: '#94a3b8' } }}
              />
              <Tooltip content={<CustomTooltip />} />
              <Legend wrapperStyle={{ fontSize: '11px' }} />
              
              {Object.values(variables).filter(v => v.enabled).map((variable) => (
                <Line
                  key={variable.key}
                  type="monotone"
                  dataKey={variable.key}
                  stroke={variable.color}
                  strokeWidth={2}
                  dot={false}
                  name={variable.key}
                  connectNulls={true}
                />
              ))}
            </LineChart>
          </ResponsiveContainer>
        )}
      </div>

      {/* Time Range Selector */}
      <div className="bg-slate-900/50 rounded-xl border border-slate-800 p-3">
        <div className="flex items-center gap-3">
          <p className="text-[10px] uppercase font-bold text-slate-500">Time Range</p>
          <div className="flex gap-2">
            {['15m', '1h', '4h', '12h', '24h'].map(range => (
              <button
                key={range}
                onClick={() => setTimeRange(range)}
                className={`px-3 py-1.5 text-xs rounded-md transition-colors ${
                  timeRange === range
                    ? 'bg-indigo-600 text-white'
                    : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
                }`}
              >
                {range}
              </button>
            ))}
          </div>
          {trendsData.length > 0 && (
            <span className="ml-auto text-xs text-slate-500">
              📊 {chartData.length} data points from {trendsData.length} records
            </span>
          )}
        </div>
      </div>

      {/* Variables Table */}
      <div className="bg-slate-900/50 rounded-xl border border-slate-800 overflow-hidden">
        <div className="bg-slate-900/80 px-3 py-1.5 border-b border-slate-800">
          <h3 className="font-bold text-slate-200 text-xs tracking-wide">Select Variables</h3>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b border-slate-800 text-slate-500">
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Enable</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Variable</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Type</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Y-Min</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Y-Max</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Auto</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Color</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/50">
              {allTags.map(({ tag, field, type }) => {
                const key = `${tag}:${field}`;
                const varConfig = variables[key];
                if (!varConfig) return null;
                
                const typeColor: Record<string, string> = {
                  'CV': 'text-indigo-400',
                  'MV': 'text-amber-400',
                  'DV': 'text-pink-400'
                };
                
                return (
                  <tr key={key} className="hover:bg-slate-800/30 transition-colors">
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        checked={varConfig.enabled}
                        onChange={() => toggleVariable(key)}
                        className="w-4 h-4 rounded border-slate-600 bg-slate-800 text-indigo-600 focus:ring-indigo-500 focus:ring-offset-slate-900"
                      />
                    </td>
                    <td className="px-3 py-2 font-mono text-slate-300">{key}</td>
                    <td className={`px-3 py-2 font-bold ${typeColor[type]}`}>{type}</td>
                    <td className="px-3 py-2">
                      <input
                        type="text"
                        disabled={varConfig.autoScale}
                        value={varConfig.yMin !== null ? varConfig.yMin : ''}
                        onChange={e => updateVariableConfig(key, 'yMin', e.target.value === '' ? null : parseFloat(e.target.value))}
                        className="w-20 bg-slate-950 text-slate-300 text-right px-2 py-0.5 text-xs rounded border border-slate-700 disabled:opacity-30 disabled:text-slate-500"
                      />
                    </td>
                    <td className="px-3 py-2">
                      <input
                        type="text"
                        disabled={varConfig.autoScale}
                        value={varConfig.yMax !== null ? varConfig.yMax : ''}
                        onChange={e => updateVariableConfig(key, 'yMax', e.target.value === '' ? null : parseFloat(e.target.value))}
                        className="w-20 bg-slate-950 text-slate-300 text-right px-2 py-0.5 text-xs rounded border border-slate-700 disabled:opacity-30 disabled:text-slate-500"
                      />
                    </td>
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        checked={varConfig.autoScale}
                        onChange={e => updateVariableConfig(key, 'autoScale', e.target.checked)}
                        className="w-4 h-4 rounded border-slate-600 bg-slate-800 text-emerald-600 focus:ring-emerald-500 focus:ring-offset-slate-900"
                      />
                    </td>
                    <td className="px-3 py-2">
                      <div 
                        className="w-6 h-6 rounded border border-slate-600"
                        style={{ backgroundColor: varConfig.color }}
                      />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
