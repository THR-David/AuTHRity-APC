import React, { useMemo, useState, useEffect, useCallback, useRef, useDeferredValue } from 'react';
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
  ResponsiveContainer,
  ReferenceLine,
  ReferenceArea,
  Brush,
} from 'recharts';
import { apiFetch } from '../lib/api';

interface TrendsTabProps {
  cvList: string[];
  mvList: string[];
  dvList: string[];
  descriptions?: Record<string, string>;
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
  type: 'CV' | 'MV' | 'DV';
  axis: 'left' | 'right';
  alias: string;
  unit: string;
  pinned: boolean;
  source: 'raw' | 'derived';
  pane: 'cv' | 'mv' | 'dv';
}

type TimeRange = '15m' | '1h' | '4h' | '12h' | '24h';
type TrendPreset = 'APC' | 'CV' | 'MV' | 'DV' | 'ALL';
type SeriesViewMode = 'core' | 'diagnostics' | 'limits' | 'all';
type WindowMode = 'relative' | 'custom';
type SamplingMode = 'all' | 'best_fit';

interface ChartRow {
  ts: number;
  timestamp: string;
  [key: string]: string | number | undefined;
}

interface TagDef {
  key: string;
  tag: string;
  field: string;
  type: 'CV' | 'MV' | 'DV';
  pane: 'cv' | 'mv' | 'dv';
  source: 'raw' | 'derived';
}

interface SeriesRowData extends TagDef {
  variable: VariableConfig;
  field: string;
  base: string;
}

interface TrendEvent {
  ts: number;
  label: string;
  color: string;
}

interface SavedPrefs {
  timeRange?: TimeRange;
  maxPoints?: number;
  showEvents?: boolean;
  maxVisiblePerPane?: number;
  showLegend?: boolean;
  showPointMarkers?: boolean;
  samplingMode?: SamplingMode;
  variables?: Record<string, Partial<VariableConfig>>;
}

interface CustomTooltipEntry {
  dataKey?: string | number;
  value?: number;
  color?: string;
  name?: string;
  payload?: Record<string, unknown>;
}

interface CustomTooltipProps {
  active?: boolean;
  payload?: readonly CustomTooltipEntry[];
  label?: string | number;
}

const formatTime24 = (value: number): string =>
  new Date(value).toLocaleTimeString([], {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });

const formatDateTime24 = (value: number): string =>
  new Date(value).toLocaleString([], {
    hour12: false,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });

const getRangeDurationMs = (range: TimeRange): number => {
  switch (range) {
    case '15m':
      return 15 * 60 * 1000;
    case '1h':
      return 60 * 60 * 1000;
    case '4h':
      return 4 * 60 * 60 * 1000;
    case '12h':
      return 12 * 60 * 60 * 1000;
    case '24h':
      return 24 * 60 * 60 * 1000;
    default:
      return 60 * 60 * 1000;
  }
};

const getBestFitBucketMs = (windowDurationMs: number): number => {
  if (windowDurationMs <= 15 * 60 * 1000) return 10_000;   // 15m -> 10s
  if (windowDurationMs <= 60 * 60 * 1000) return 60_000;   // 1h -> 1m
  if (windowDurationMs <= 4 * 60 * 60 * 1000) return 120_000;
  if (windowDurationMs <= 12 * 60 * 60 * 1000) return 300_000;
  return 600_000;
};

const formatDateTimeLocalInput = (valueMs: number): string => {
  const date = new Date(valueMs);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
};

const parseDateTimeLocalInput = (value: string): number | null => {
  const normalized = value.trim().replace(' ', 'T');
  const ms = new Date(normalized).getTime();
  return Number.isFinite(ms) ? ms : null;
};

const formatTooltipTimestamp = (label: string | number | undefined): string => {
  if (typeof label === 'number' && Number.isFinite(label)) {
    return formatDateTime24(label);
  }

  if (typeof label === 'string') {
    const asNumber = Number(label);
    if (Number.isFinite(asNumber)) {
      return formatDateTime24(asNumber);
    }

    const parsed = Date.parse(label);
    if (!Number.isNaN(parsed)) {
      return formatDateTime24(parsed);
    }

    return label;
  }

  return '';
};

const COLORS = ['#4f46e5', '#0891b2', '#dc2626', '#16a34a', '#f59e0b', '#9333ea', '#ea580c', '#2563eb', '#db2777'];
const PREFS_KEY = 'authrity.trendstab.v4';

// Keep short ranges responsive and prevent large-range over-fetch.
const getWindowPreloadFactor = (range: TimeRange): number => {
  switch (range) {
    case '15m':
      return 0.75;
    case '1h':
      return 0.35;
    case '4h':
      return 0.2;
    case '12h':
      return 0.1;
    case '24h':
      return 0.05;
    default:
      return 0.25;
  }
};
const VIRTUAL_ROW_HEIGHT = 36;
const VIRTUAL_OVERSCAN = 10;
const SERIES_TABLE_HEIGHT = 460;
const TYPE_COLOR: Record<'CV' | 'MV' | 'DV', string> = {
  CV: 'text-indigo-400',
  MV: 'text-amber-400',
  DV: 'text-cyan-400',
};

const loadSavedPrefs = (): SavedPrefs => {
  if (typeof window === 'undefined') return {};
  try {
    const raw = window.localStorage.getItem(PREFS_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as SavedPrefs;
    return parsed || {};
  } catch {
    return {};
  }
};

interface TrendChartsProps {
  chartData: ChartRow[];
  loading: boolean;
  showEvents: boolean;
  showLegend: boolean;
  showPointMarkers: boolean;
  modeEvents: TrendEvent[];
  activeMpcBands: Array<{ x1: number; x2: number }>;
  visiblePaneVariables: {
    cv: VariableConfig[];
    mv: VariableConfig[];
    dv: VariableConfig[];
  };
  paneVariables: {
    cv: VariableConfig[];
    mv: VariableConfig[];
    dv: VariableConfig[];
  };
  paneDomains: Record<'cv' | 'mv' | 'dv', { left: [number | 'auto', number | 'auto']; right: [number | 'auto', number | 'auto'] }>;
  variables: Record<string, VariableConfig>;
}

const TrendCharts = React.memo(({
  chartData,
  loading,
  showEvents,
  showLegend,
  showPointMarkers,
  modeEvents,
  activeMpcBands,
  visiblePaneVariables,
  paneVariables,
  paneDomains,
  variables,
}: TrendChartsProps) => {
  const tooltipContent = useCallback((props: CustomTooltipProps) => {
    const { active, payload, label } = props;
    if (!active || !payload || payload.length === 0) return null;
    const labelText = formatTooltipTimestamp(label);

    return (
      <div style={{ backgroundColor: '#0f172a', border: '1px solid #334155', borderRadius: '8px', padding: '8px', fontSize: '11px' }}>
        <p style={{ color: '#cbd5e1', marginBottom: '4px' }}>{labelText}</p>
        {payload.map(entry => {
          const variableKey = String(entry.dataKey);
          const rawValue = entry.payload?.[`${variableKey}__raw`] ?? entry.value;
          const variable = variables[variableKey];

          return (
            <p key={variableKey} style={{ color: entry.color ?? '#e2e8f0', margin: '2px 0' }}>
              {(variable?.alias || entry.name || variableKey)}: {typeof rawValue === 'number' ? rawValue.toFixed(3) : 'N/A'}{variable?.unit ? ` ${variable.unit}` : ''}
              {variable ? ` [${variable.axis}]` : ''}
            </p>
          );
        })}
      </div>
    );
  }, [variables]);

  return (
    <div className="space-y-3">
      {loading && (
        <div className="text-[11px] text-slate-400">Updating trend data...</div>
      )}
      {(['cv', 'mv', 'dv'] as const).map((pane, paneIndex) => {
        const vars = visiblePaneVariables[pane];
        const totalVars = paneVariables[pane].length;
        if (totalVars === 0) return null;

        return (
          <div key={pane} className="rounded-lg border border-slate-800 bg-slate-950/40 p-2">
            <div className="mb-1 flex items-center gap-2">
              <p className="text-[10px] uppercase tracking-wide text-slate-400">
              {pane === 'cv' ? 'CV Performance' : pane === 'mv' ? 'MV Moves and Modes' : 'DV and External Disturbances'}
              </p>
              <span className="text-[10px] text-slate-500">
                showing {vars.length}/{totalVars} lines
              </span>
            </div>

            <ResponsiveContainer width="100%" height={330}>
              <LineChart data={chartData} syncId="apc-trends">
                <CartesianGrid strokeDasharray="3 3" stroke="#334155" />

                {activeMpcBands.map((segment, idx) => (
                  <ReferenceArea
                    key={`${pane}-band-${idx}`}
                    x1={segment.x1}
                    x2={segment.x2}
                    yAxisId="left"
                    strokeOpacity={0}
                    fill="#22c55e"
                    fillOpacity={0.08}
                  />
                ))}

                {showEvents && modeEvents.map((event, idx) => (
                  <ReferenceLine
                    key={`${pane}-ev-${idx}`}
                    x={event.ts}
                    stroke={event.color}
                    strokeDasharray="4 4"
                    label={paneIndex === 0 ? { value: event.label, position: 'insideTopLeft', fill: '#94a3b8', fontSize: 9 } : undefined}
                  />
                ))}

                <XAxis
                  dataKey="ts"
                  stroke="#94a3b8"
                  tick={{ fontSize: 10 }}
                  tickFormatter={value => formatTime24(Number(value))}
                  hide={paneIndex < 2}
                />

                <YAxis
                  yAxisId="left"
                  stroke="#94a3b8"
                  tick={{ fontSize: 10 }}
                  domain={paneDomains[pane].left}
                  label={{
                    value: 'Left Axis',
                    angle: -90,
                    position: 'insideLeft',
                    style: { fontSize: 10, fill: '#94a3b8' },
                  }}
                />

                <YAxis
                  yAxisId="right"
                  orientation="right"
                  stroke="#94a3b8"
                  tick={{ fontSize: 10 }}
                  domain={paneDomains[pane].right}
                />

                <Tooltip content={tooltipContent} />
                {showLegend && (
                  <Legend
                    wrapperStyle={{ fontSize: '10px', maxHeight: '44px', overflowY: 'auto', lineHeight: '14px' }}
                    formatter={(value: string) => (value.length > 28 ? `${value.slice(0, 28)}...` : value)}
                  />
                )}

                {vars.map(variable => (
                  <Line
                    key={variable.key}
                    type="linear"
                    dataKey={variable.key}
                    yAxisId={variable.axis}
                    stroke={variable.color}
                    strokeWidth={variable.pinned ? 2.8 : 1.9}
                    opacity={0.8}
                    dot={showPointMarkers ? (dotProps => {
                      const payload = dotProps?.payload as Record<string, unknown> | undefined;
                      if (!payload || payload.__rawPoint !== 1) return null;
                      return (
                        <circle
                          cx={dotProps.cx}
                          cy={dotProps.cy}
                          r={2}
                          fill={variable.color}
                          stroke="#0f172a"
                          strokeWidth={1}
                        />
                      );
                    }) : false}
                    name={`${variable.alias}${variable.unit ? ` (${variable.unit})` : ''}`}
                    connectNulls={false}
                    strokeDasharray={variable.key.includes('Limit') ? '5 4' : undefined}
                  />
                ))}

                {paneIndex === 2 && (
                  <Brush
                    dataKey="ts"
                    height={22}
                    stroke="#64748b"
                    tickFormatter={value => formatTime24(Number(value))}
                  />
                )}
              </LineChart>
            </ResponsiveContainer>
          </div>
        );
      })}
    </div>
  );
});

interface SeriesConfigTableProps {
  seriesRows: SeriesRowData[];
  seriesViewMode: SeriesViewMode;
  seriesTypeFilter: 'ALL' | 'CV' | 'MV' | 'DV';
  seriesSearch: string;
  onSetSeriesViewMode: (mode: SeriesViewMode) => void;
  onSetSeriesTypeFilter: (value: 'ALL' | 'CV' | 'MV' | 'DV') => void;
  onSetSeriesSearch: (value: string) => void;
  onToggleVariable: (key: string) => void;
  onUpdateVariableConfig: (
    key: string,
    field: 'yMin' | 'yMax' | 'autoScale' | 'axis' | 'alias' | 'unit' | 'pinned',
    value: number | boolean | 'left' | 'right' | string | null
  ) => void;
}

interface SeriesDraft {
  alias: string;
  unit: string;
  yMin: string;
  yMax: string;
}

const SeriesConfigTable = React.memo(({
  seriesRows,
  seriesViewMode,
  seriesTypeFilter,
  seriesSearch,
  onSetSeriesViewMode,
  onSetSeriesTypeFilter,
  onSetSeriesSearch,
  onToggleVariable,
  onUpdateVariableConfig,
}: SeriesConfigTableProps) => {
  const [scrollTop, setScrollTop] = useState(0);
  const [drafts, setDrafts] = useState<Record<string, SeriesDraft>>({});

  useEffect(() => {
    setScrollTop(0);
  }, [seriesSearch, seriesTypeFilter, seriesViewMode]);

  useEffect(() => {
    setDrafts(prev => {
      const next = { ...prev };
      let changed = false;
      const keys = new Set(seriesRows.map(row => row.key));

      Object.keys(next).forEach(key => {
        if (!keys.has(key)) {
          delete next[key];
          changed = true;
        }
      });

      seriesRows.forEach(row => {
        if (!next[row.key]) {
          next[row.key] = {
            alias: row.variable.alias,
            unit: row.variable.unit,
            yMin: row.variable.yMin === null ? '' : String(row.variable.yMin),
            yMax: row.variable.yMax === null ? '' : String(row.variable.yMax),
          };
          changed = true;
        }
      });

      return changed ? next : prev;
    });
  }, [seriesRows]);

  const parseNumberOrNull = useCallback((value: string): number | null => {
    if (value.trim() === '') return null;
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }, []);

  const updateDraft = useCallback((key: string, field: keyof SeriesDraft, value: string) => {
    setDrafts(prev => {
      const existing = prev[key] ?? { alias: '', unit: '', yMin: '', yMax: '' };
      if (existing[field] === value) return prev;
      return {
        ...prev,
        [key]: {
          ...existing,
          [field]: value,
        },
      };
    });
  }, []);

  const commitField = useCallback((row: SeriesRowData, field: keyof SeriesDraft) => {
    const draft = drafts[row.key];
    if (!draft) return;

    if (field === 'alias') {
      onUpdateVariableConfig(row.key, 'alias', draft.alias);
      return;
    }
    if (field === 'unit') {
      onUpdateVariableConfig(row.key, 'unit', draft.unit);
      return;
    }
    if (field === 'yMin') {
      onUpdateVariableConfig(row.key, 'yMin', parseNumberOrNull(draft.yMin));
      return;
    }
    if (field === 'yMax') {
      onUpdateVariableConfig(row.key, 'yMax', parseNumberOrNull(draft.yMax));
    }
  }, [drafts, onUpdateVariableConfig, parseNumberOrNull]);

  const commitOnEnter = useCallback((e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.currentTarget.blur();
    }
  }, []);

  const totalRows = seriesRows.length;
  const startIndex = Math.max(0, Math.floor(scrollTop / VIRTUAL_ROW_HEIGHT) - VIRTUAL_OVERSCAN);
  const visibleCount = Math.ceil(SERIES_TABLE_HEIGHT / VIRTUAL_ROW_HEIGHT) + VIRTUAL_OVERSCAN * 2;
  const endIndex = Math.min(totalRows, startIndex + visibleCount);
  const topSpacer = startIndex * VIRTUAL_ROW_HEIGHT;
  const bottomSpacer = Math.max(0, (totalRows - endIndex) * VIRTUAL_ROW_HEIGHT);
  const visibleRows = useMemo(() => seriesRows.slice(startIndex, endIndex), [endIndex, seriesRows, startIndex]);

  return (
    <div className="bg-slate-900/50 rounded-xl border border-slate-800 overflow-hidden">
      <div className="bg-slate-900/80 px-3 py-1.5 border-b border-slate-800">
        <h3 className="font-bold text-slate-200 text-xs tracking-wide">Series Configuration</h3>
      </div>
      <div className="border-b border-slate-800 px-3 py-2 flex flex-wrap items-center gap-2">
        <div className="flex gap-1">
          {(['core', 'diagnostics', 'limits', 'all'] as SeriesViewMode[]).map(mode => (
            <button
              key={mode}
              onClick={() => onSetSeriesViewMode(mode)}
              className={`px-2.5 py-1 text-xs rounded-md transition-colors ${
                seriesViewMode === mode ? 'bg-indigo-600 text-white' : 'bg-slate-800 text-slate-300 hover:bg-slate-700'
              }`}
            >
              {mode}
            </button>
          ))}
        </div>

        <select
          value={seriesTypeFilter}
          onChange={e => onSetSeriesTypeFilter(e.target.value as 'ALL' | 'CV' | 'MV' | 'DV')}
          className="bg-slate-950 text-slate-200 border border-slate-700 rounded px-2 py-1 text-xs"
        >
          <option value="ALL">All types</option>
          <option value="CV">CV</option>
          <option value="MV">MV</option>
          <option value="DV">DV</option>
        </select>

        <input
          type="text"
          value={seriesSearch}
          onChange={e => onSetSeriesSearch(e.target.value)}
          placeholder="Search variable or alias"
          className="w-64 bg-slate-950 text-slate-200 border border-slate-700 rounded px-2 py-1 text-xs"
        />

        <span className="ml-auto text-xs text-slate-500">{seriesRows.length} rows</span>
      </div>
      <div className="overflow-x-auto">
        <div
          className="overflow-y-auto"
          style={{ maxHeight: `${SERIES_TABLE_HEIGHT}px` }}
          onScroll={e => setScrollTop(e.currentTarget.scrollTop)}
        >
          <table className="w-full text-left text-xs">
            <thead className="sticky top-0 bg-slate-900 z-10">
              <tr className="border-b border-slate-800 text-slate-500">
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Enable</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Variable</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Alias</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Unit</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Type</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Axis</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Y-Min</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Y-Max</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Auto</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Pin</th>
                <th className="px-3 py-2 font-bold uppercase tracking-wider text-[10px]">Color</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/50">
              {topSpacer > 0 && (
                <tr>
                  <td colSpan={11} style={{ height: `${topSpacer}px` }} />
                </tr>
              )}
              {visibleRows.map(row => {
                const { key, type, source, variable } = row;
                const draft = drafts[key] ?? {
                  alias: variable.alias,
                  unit: variable.unit,
                  yMin: variable.yMin === null ? '' : String(variable.yMin),
                  yMax: variable.yMax === null ? '' : String(variable.yMax),
                };

                return (
                  <tr key={key} className="hover:bg-slate-800/30 transition-colors" style={{ height: `${VIRTUAL_ROW_HEIGHT}px` }}>
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        checked={variable.enabled}
                        onChange={() => onToggleVariable(key)}
                        className="w-4 h-4 rounded border-slate-600 bg-slate-800 text-indigo-600 focus:ring-indigo-500 focus:ring-offset-slate-900"
                      />
                    </td>
                    <td className="px-3 py-2 font-mono text-slate-300">{key}</td>
                    <td className="px-3 py-2">
                      <input
                        type="text"
                        value={draft.alias}
                        onChange={e => updateDraft(key, 'alias', e.target.value)}
                        onBlur={() => commitField(row, 'alias')}
                        onKeyDown={commitOnEnter}
                        className="w-44 bg-slate-950 text-slate-300 px-2 py-0.5 text-xs rounded border border-slate-700"
                      />
                    </td>
                    <td className="px-3 py-2">
                      <input
                        type="text"
                        value={draft.unit}
                        onChange={e => updateDraft(key, 'unit', e.target.value)}
                        onBlur={() => commitField(row, 'unit')}
                        onKeyDown={commitOnEnter}
                        className="w-20 bg-slate-950 text-slate-300 px-2 py-0.5 text-xs rounded border border-slate-700"
                      />
                    </td>
                    <td className={`px-3 py-2 font-bold ${TYPE_COLOR[type]}`}>{type}</td>
                    <td className="px-3 py-2">
                      <select
                        disabled={source === 'derived'}
                        value={variable.axis}
                        onChange={e => onUpdateVariableConfig(key, 'axis', e.target.value as 'left' | 'right')}
                        className="bg-slate-950 text-slate-300 px-2 py-0.5 text-xs rounded border border-slate-700 disabled:opacity-30"
                      >
                        <option value="left">left</option>
                        <option value="right">right</option>
                      </select>
                    </td>
                    <td className="px-3 py-2">
                      <input
                        type="text"
                        disabled={variable.autoScale}
                        value={draft.yMin}
                        onChange={e => updateDraft(key, 'yMin', e.target.value)}
                        onBlur={() => commitField(row, 'yMin')}
                        onKeyDown={commitOnEnter}
                        className="w-20 bg-slate-950 text-slate-300 text-right px-2 py-0.5 text-xs rounded border border-slate-700 disabled:opacity-30 disabled:text-slate-500"
                      />
                    </td>
                    <td className="px-3 py-2">
                      <input
                        type="text"
                        disabled={variable.autoScale}
                        value={draft.yMax}
                        onChange={e => updateDraft(key, 'yMax', e.target.value)}
                        onBlur={() => commitField(row, 'yMax')}
                        onKeyDown={commitOnEnter}
                        className="w-20 bg-slate-950 text-slate-300 text-right px-2 py-0.5 text-xs rounded border border-slate-700 disabled:opacity-30 disabled:text-slate-500"
                      />
                    </td>
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        checked={variable.autoScale}
                        onChange={e => onUpdateVariableConfig(key, 'autoScale', e.target.checked)}
                        className="w-4 h-4 rounded border-slate-600 bg-slate-800 text-emerald-600 focus:ring-emerald-500 focus:ring-offset-slate-900"
                      />
                    </td>
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        checked={variable.pinned}
                        onChange={e => onUpdateVariableConfig(key, 'pinned', e.target.checked)}
                        className="w-4 h-4 rounded border-slate-600 bg-slate-800 text-cyan-600 focus:ring-cyan-500 focus:ring-offset-slate-900"
                      />
                    </td>
                    <td className="px-3 py-2">
                      <div
                        className="w-6 h-6 rounded border border-slate-600"
                        style={{ backgroundColor: variable.color }}
                      />
                    </td>
                  </tr>
                );
              })}
              {bottomSpacer > 0 && (
                <tr>
                  <td colSpan={11} style={{ height: `${bottomSpacer}px` }} />
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
});

export const TrendsTab: React.FC<TrendsTabProps> = ({ cvList, mvList, dvList, descriptions = {} }) => {
  const prefsRef = useRef<SavedPrefs>(loadSavedPrefs());

  const [timeRange, setTimeRange] = useState<TimeRange>(prefsRef.current.timeRange ?? '1h');
  const [windowMode, setWindowMode] = useState<WindowMode>('relative');
  const [liveAutoRefresh, setLiveAutoRefresh] = useState<boolean>(true);
  const [windowEndMs, setWindowEndMs] = useState<number>(Date.now());
  const [customStartInput, setCustomStartInput] = useState<string>('');
  const [customEndInput, setCustomEndInput] = useState<string>('');
  const [customWindowStartMs, setCustomWindowStartMs] = useState<number | null>(null);
  const [customWindowEndMs, setCustomWindowEndMs] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [trendsData, setTrendsData] = useState<TrendsDataPoint[]>([]);
  const [chartData, setChartData] = useState<ChartRow[]>([]);
  const [fullData, setFullData] = useState<ChartRow[]>([]);
  const [bufferStartMs, setBufferStartMs] = useState<number | null>(null);
  const [bufferEndMs, setBufferEndMs] = useState<number | null>(null);
  const [variables, setVariables] = useState<Record<string, VariableConfig>>({});
  const [maxPoints, setMaxPoints] = useState<number>(prefsRef.current.maxPoints ?? 1200);
  const [showEvents, setShowEvents] = useState<boolean>(prefsRef.current.showEvents ?? true);
  const [errorText, setErrorText] = useState<string>('');
  const [authError, setAuthError] = useState(false);
  const [seriesViewMode, setSeriesViewMode] = useState<SeriesViewMode>('core');
  const [seriesTypeFilter, setSeriesTypeFilter] = useState<'ALL' | 'CV' | 'MV' | 'DV'>('ALL');
  const [seriesSearch, setSeriesSearch] = useState<string>('');
  const [maxVisiblePerPane, setMaxVisiblePerPane] = useState<number>(prefsRef.current.maxVisiblePerPane ?? 10);
  const [showLegend, setShowLegend] = useState<boolean>(prefsRef.current.showLegend ?? false);
  const [showPointMarkers, setShowPointMarkers] = useState<boolean>(prefsRef.current.showPointMarkers ?? false);
  const [samplingMode, setSamplingMode] = useState<SamplingMode>(prefsRef.current.samplingMode ?? 'best_fit');

  useEffect(() => {
    const now = Date.now();
    const defaultStart = now - getRangeDurationMs(timeRange);
    setCustomStartInput(formatDateTimeLocalInput(defaultStart));
    setCustomEndInput(formatDateTimeLocalInput(now));
    setCustomWindowStartMs(defaultStart);
    setCustomWindowEndMs(now);
  }, []);

  useEffect(() => {
    if (windowMode !== 'relative') return;
    const duration = getRangeDurationMs(timeRange);
    const start = windowEndMs - duration;
    setCustomStartInput(formatDateTimeLocalInput(start));
    setCustomEndInput(formatDateTimeLocalInput(windowEndMs));
  }, [timeRange, windowEndMs, windowMode]);

  const allTags = useMemo<TagDef[]>(() => {
    const cvFields = ['PV', 'Target', 'Prediction', 'LowLimit', 'HighLimit'];
    const mvFields = ['OP', 'SP', 'Mode', 'ModeTarget', 'LowLimit', 'HighLimit'];

    const tags: TagDef[] = [];

    cvList.forEach(tag => {
      cvFields.forEach(field => {
        tags.push({ key: `${tag}:${field}`, tag, field, type: 'CV', pane: 'cv', source: 'raw' });
      });
      tags.push({ key: `${tag}:Error`, tag, field: 'Error', type: 'CV', pane: 'cv', source: 'derived' });
      tags.push({ key: `${tag}:ConstraintDistance`, tag, field: 'ConstraintDistance', type: 'CV', pane: 'cv', source: 'derived' });
    });

    mvList.forEach(tag => {
      mvFields.forEach(field => {
        tags.push({ key: `${tag}:${field}`, tag, field, type: 'MV', pane: 'mv', source: 'raw' });
      });
      tags.push({ key: `${tag}:SaturationFlag`, tag, field: 'SaturationFlag', type: 'MV', pane: 'mv', source: 'derived' });
    });

    dvList.forEach(tag => {
      tags.push({ key: `${tag}:PV`, tag, field: 'PV', type: 'DV', pane: 'dv', source: 'raw' });
    });

    return tags;
  }, [cvList, mvList, dvList]);

  useEffect(() => {
    setVariables(prev => {
      const next: Record<string, VariableConfig> = {};
      allTags.forEach((tagDef, idx) => {
        const { key, tag, field, type, pane, source } = tagDef;
        const existing = prev[key];
        const saved = prefsRef.current.variables?.[key];
        const defaultAlias = field === 'PV'
          ? descriptions[tag] || tag
          : `${descriptions[tag] || tag} ${field}`;

        next[key] =
          existing ?? {
            key,
            enabled: saved?.enabled ?? (
              (type === 'CV' && field === 'PV') ||
              (type === 'MV' && field === 'OP') ||
              (type === 'DV' && field === 'PV')
            ),
            yMin: null,
            yMax: null,
            autoScale: saved?.autoScale ?? true,
            color: COLORS[idx % COLORS.length],
            type,
            axis: (saved?.axis as 'left' | 'right' | undefined) ?? (type === 'MV' ? 'right' : 'left'),
            alias: saved?.alias ?? defaultAlias,
            unit: saved?.unit ?? '',
            pinned: saved?.pinned ?? false,
            source,
            pane,
          };
      });
      return next;
    });
  }, [allTags, descriptions]);

  const enabledVariables = useMemo(
    () => Object.values(variables).filter(v => v.enabled),
    [variables]
  );

  const paneVariables = useMemo(() => {
    const scoreFor = (v: VariableConfig): number => {
      let score = v.pinned ? 100 : 0;
      if (v.key.endsWith(':PV') || v.key.endsWith(':Target') || v.key.endsWith(':OP') || v.key.endsWith(':SP')) score += 20;
      if (v.key.endsWith(':Prediction')) score += 10;
      if (v.key.includes('Limit')) score -= 20;
      return score;
    };

    const sorted = [...enabledVariables].sort((a, b) => scoreFor(b) - scoreFor(a));
    return {
      cv: sorted.filter(v => v.pane === 'cv'),
      mv: sorted.filter(v => v.pane === 'mv'),
      dv: sorted.filter(v => v.pane === 'dv'),
    };
  }, [enabledVariables]);

  const visiblePaneVariables = useMemo(() => ({
    cv: paneVariables.cv.slice(0, maxVisiblePerPane),
    mv: paneVariables.mv.slice(0, maxVisiblePerPane),
    dv: paneVariables.dv.slice(0, maxVisiblePerPane),
  }), [maxVisiblePerPane, paneVariables]);

  const enabledKeySignature = useMemo(
    () => enabledVariables.map(v => v.key).sort().join(','),
    [enabledVariables]
  );

  const toggleVariable = useCallback((key: string) => {
    setVariables(prev => ({
      ...prev,
      [key]: { ...prev[key], enabled: !prev[key].enabled }
    }));
  }, []);

  const updateVariableConfig = useCallback((
    key: string,
    field: 'yMin' | 'yMax' | 'autoScale' | 'axis' | 'alias' | 'unit' | 'pinned',
    value: number | boolean | 'left' | 'right' | string | null
  ) => {
    setVariables(prev => {
      const existing = prev[key];
      if (!existing) return prev;
      if (existing[field] === value) return prev;

      return {
        ...prev,
        [key]: { ...existing, [field]: value }
      };
    });
  }, []);

  const setEnabledByType = (type: 'CV' | 'MV' | 'DV' | 'ALL' | 'NONE') => {
    setVariables(prev => {
      const next = { ...prev };
      Object.values(next).forEach(variable => {
        if (type === 'ALL') {
          variable.enabled = true;
        } else if (type === 'NONE') {
          variable.enabled = false;
        } else {
          variable.enabled = variable.type === type;
        }
      });
      return next;
    });
  };

  const applyPreset = (preset: TrendPreset) => {
    setVariables(prev => {
      const next = { ...prev };

      Object.values(next).forEach(v => {
        v.enabled = false;
      });

      Object.values(next).forEach(v => {
        const isCvCore = v.type === 'CV' && ['PV', 'Target', 'Prediction'].some(s => v.key.endsWith(`:${s}`));
        const isMvCore = v.type === 'MV' && ['OP', 'SP', 'Mode', 'ModeTarget'].some(s => v.key.endsWith(`:${s}`));
        const isDvCore = v.type === 'DV' && v.key.endsWith(':PV');
        if (preset === 'ALL') v.enabled = true;
        if (preset === 'CV' && (isCvCore || v.key.endsWith(':Error'))) v.enabled = true;
        if (preset === 'MV' && (isMvCore || v.key.endsWith(':SaturationFlag'))) v.enabled = true;
        if (preset === 'DV' && isDvCore) v.enabled = true;
        if (preset === 'APC') {
          // APC default should remain readable on large plants: only key control signals.
          const apcMinimalCv = v.type === 'CV' && ['PV', 'Target'].some(s => v.key.endsWith(`:${s}`));
          const apcMinimalMv = v.type === 'MV' && ['OP', 'SP', 'Mode'].some(s => v.key.endsWith(`:${s}`));
          const apcMinimalDv = v.type === 'DV' && v.key.endsWith(':PV');
          v.enabled = apcMinimalCv || apcMinimalMv || apcMinimalDv;
        }
      });

      return next;
    });
  };

  const getFieldName = (key: string): string => key.split(':').slice(1).join(':');
  const getBaseTag = (key: string): string => key.split(':')[0];
  const isLimitField = (field: string): boolean => field === 'LowLimit' || field === 'HighLimit';
  const isDiagnosticField = (field: string): boolean => field === 'Error' || field === 'ConstraintDistance' || field === 'SaturationFlag';
  const isCoreField = (field: string): boolean =>
    ['PV', 'Target', 'Prediction', 'OP', 'SP', 'Mode', 'ModeTarget'].includes(field);
  const deferredSeriesSearch = useDeferredValue(seriesSearch);

  const seriesRows = useMemo<SeriesRowData[]>(() => {
    const query = deferredSeriesSearch.trim().toLowerCase();
    const rows = allTags
      .map(tag => {
        const variable = variables[tag.key];
        if (!variable) return null;

        const field = getFieldName(tag.key);
        const base = getBaseTag(tag.key);
        return { ...tag, variable, field, base };
      })
      .filter((v): v is NonNullable<typeof v> => Boolean(v))
      .filter(row => {
        if (seriesTypeFilter !== 'ALL' && row.type !== seriesTypeFilter) return false;

        if (seriesViewMode === 'core' && !isCoreField(row.field)) return false;
        if (seriesViewMode === 'diagnostics' && !isDiagnosticField(row.field)) return false;
        if (seriesViewMode === 'limits' && !isLimitField(row.field)) return false;

        if (query) {
          const haystack = `${row.key} ${row.variable.alias} ${row.base} ${row.field}`.toLowerCase();
          return haystack.includes(query);
        }

        return true;
      })
      .sort((a, b) => {
        const typeCmp = a.type.localeCompare(b.type);
        if (typeCmp !== 0) return typeCmp;
        const baseCmp = a.base.localeCompare(b.base);
        if (baseCmp !== 0) return baseCmp;
        return a.field.localeCompare(b.field);
      });

    return rows;
  }, [allTags, deferredSeriesSearch, seriesTypeFilter, seriesViewMode, variables]);

  const currentWindow = useMemo((): { startMs: number; endMs: number } => {
    if (windowMode === 'custom' && customWindowStartMs !== null && customWindowEndMs !== null && customWindowEndMs > customWindowStartMs) {
      return {
        startMs: customWindowStartMs,
        endMs: customWindowEndMs,
      };
    }

    const duration = getRangeDurationMs(timeRange);
    const end = windowEndMs;
    const start = end - duration;
    return {
      startMs: start,
      endMs: end,
    };
  }, [customWindowEndMs, customWindowStartMs, timeRange, windowEndMs, windowMode]);

  const getRowsInWindow = useCallback(
    (rows: ChartRow[], startMs: number, endMs: number): ChartRow[] =>
      rows.filter(row => row.ts >= startMs && row.ts <= endMs),
    []
  );

  const shiftWindow = (direction: -1 | 1) => {
    const duration = getRangeDurationMs(timeRange);
    const stepMs = Math.max(1000, Math.floor(duration / 10));
    setLiveAutoRefresh(false);

    if (windowMode === 'custom') {
      if (customWindowStartMs === null || customWindowEndMs === null || customWindowEndMs <= customWindowStartMs) return;

      const shiftedStart = customWindowStartMs + direction * stepMs;
      const shiftedEnd = customWindowEndMs + direction * stepMs;
      setCustomWindowStartMs(shiftedStart);
      setCustomWindowEndMs(shiftedEnd);
      setCustomStartInput(formatDateTimeLocalInput(shiftedStart));
      setCustomEndInput(formatDateTimeLocalInput(shiftedEnd));
      return;
    }

    const now = Date.now();
    const nextEnd = windowEndMs + direction * stepMs;
    setWindowEndMs(Math.min(nextEnd, now));
  };

  const applyCustomWindow = () => {
    const start = parseDateTimeLocalInput(customStartInput);
    const end = parseDateTimeLocalInput(customEndInput);

    if (start === null || end === null || end <= start) {
      setErrorText('Invalid custom window. End time must be after start time.');
      return;
    }

    setErrorText('');
    setLiveAutoRefresh(false);
    setCustomWindowStartMs(start);
    setCustomWindowEndMs(end);
    setWindowMode('custom');
  };

  const useLiveWindow = () => {
    const now = Date.now();
    const start = now - getRangeDurationMs(timeRange);
    setWindowEndMs(now);
    setCustomStartInput(formatDateTimeLocalInput(start));
    setCustomEndInput(formatDateTimeLocalInput(now));
    setCustomWindowStartMs(start);
    setCustomWindowEndMs(now);
    setWindowMode('relative');
    setLiveAutoRefresh(true);
  };

  const buildRawRows = (data: TrendsDataPoint[]): ChartRow[] => {
    const grouped: Record<string, ChartRow> = {};

    data.forEach(point => {
      const ts = new Date(point.timestamp).getTime();
      if (!grouped[point.timestamp]) {
        grouped[point.timestamp] = {
          ts,
          timestamp: formatTime24(ts),
          __rawPoint: 1,
        };
      }

      grouped[point.timestamp][`${point.tag}:${point.field}`] = point.value;
    });

    return Object.values(grouped).sort((a, b) => a.ts - b.ts);
  };

  const enrichDerivedRows = (rows: ChartRow[]): ChartRow[] => {
    return rows.map(row => {
      const next: ChartRow = { ...row };

      cvList.forEach(cv => {
        const pv = row[`${cv}:PV`];
        const target = row[`${cv}:Target`];
        const low = row[`${cv}:LowLimit`];
        const high = row[`${cv}:HighLimit`];

        if (typeof pv === 'number' && typeof target === 'number') {
          next[`${cv}:Error`] = pv - target;
        }

        if (typeof pv === 'number' && typeof low === 'number' && typeof high === 'number') {
          next[`${cv}:ConstraintDistance`] = Math.min(pv - low, high - pv);
        }
      });

      mvList.forEach(mv => {
        const op = row[`${mv}:OP`];
        const low = row[`${mv}:LowLimit`];
        const high = row[`${mv}:HighLimit`];
        if (typeof op !== 'number' || typeof low !== 'number' || typeof high !== 'number' || high <= low) return;

        const ratio = (op - low) / (high - low);
        next[`${mv}:SaturationFlag`] = ratio <= 0.02 || ratio >= 0.98 ? 1 : 0;
      });

      return next;
    });
  };

  const downsampleRows = (rows: ChartRow[], max: number): ChartRow[] => {
    if (rows.length <= max) return rows;
    if (max <= 3) return [rows[0], rows[Math.floor(rows.length / 2)], rows[rows.length - 1]];

    const sampled: ChartRow[] = [rows[0]];
    const stride = (rows.length - 2) / (max - 2);
    for (let i = 0; i < max - 2; i += 1) {
      const index = Math.floor(1 + i * stride);
      sampled.push(rows[index]);
    }
    sampled.push(rows[rows.length - 1]);
    return sampled;
  };

  // QuestDB points are often sparse/asynchronous across tags. Resampling onto a shared
  // timeline (last-value hold) makes non-connected rendering usable and readable.
  const resampleRowsToTimeline = (rows: ChartRow[], keys: string[], targetPoints: number): ChartRow[] => {
    if (rows.length === 0 || keys.length === 0) return rows;
    if (rows.length === 1) return rows;

    const startTs = rows[0].ts;
    const endTs = rows[rows.length - 1].ts;
    if (endTs <= startTs) return rows;

    const buckets = Math.max(2, Math.min(targetPoints, 5000));
    const stepMs = Math.max(1, Math.floor((endTs - startTs) / (buckets - 1)));

    const lastSeen: Record<string, number | undefined> = {};
    const timeline: ChartRow[] = [];
    let sourceIdx = 0;

    for (let i = 0; i < buckets; i += 1) {
      const ts = i === buckets - 1 ? endTs : startTs + i * stepMs;
      let sourceSeen = false;

      while (sourceIdx < rows.length && rows[sourceIdx].ts <= ts) {
        const src = rows[sourceIdx];
        sourceSeen = true;
        keys.forEach(key => {
          const v = src[key];
          if (typeof v === 'number' && Number.isFinite(v)) {
            lastSeen[key] = v;
          }
        });
        sourceIdx += 1;
      }

      const point: ChartRow = {
        ts,
        timestamp: formatTime24(ts),
        __rawPoint: sourceSeen ? 1 : 0,
      };

      keys.forEach(key => {
        if (lastSeen[key] !== undefined) {
          point[key] = lastSeen[key];
        }
      });

      timeline.push(point);
    }

    return timeline;
  };

  const getSeriesValues = useCallback(
    (rows: ChartRow[], key: string): number[] =>
      rows
        .map(row => row[key])
        .filter((v): v is number => typeof v === 'number' && Number.isFinite(v)),
    []
  );

  const getDisplayRows = useCallback(
    (rows: ChartRow[], activeVariables: VariableConfig[]): ChartRow[] => {
      const activeKeys = activeVariables.map(v => v.key);
      const aligned = resampleRowsToTimeline(rows, activeKeys, maxPoints);

      // Keep a final cap for safety if preferences are changed aggressively.
      return aligned.length > maxPoints ? downsampleRows(aligned, maxPoints) : aligned;
    },
    [maxPoints]
  );

  const fetchTrendsData = useCallback(async (
    requestStartMs: number,
    requestEndMs: number,
    options?: { silent?: boolean }
  ) => {
    const silent = options?.silent ?? false;

    if (enabledVariables.length === 0) {
      setTrendsData([]);
      setChartData([]);
      setFullData([]);
      setBufferStartMs(null);
      setBufferEndMs(null);
      return;
    }

    if (!silent) {
      setLoading(true);
    }
    setErrorText('');
    setAuthError(false);

    const requested = new Set<string>();
    const enabledCvBases = new Set(enabledVariables.filter(v => v.type === 'CV').map(v => v.key.split(':')[0]));
    const enabledMvBases = new Set(enabledVariables.filter(v => v.type === 'MV').map(v => v.key.split(':')[0]));

    enabledVariables.forEach(v => {
      if (v.source === 'raw') {
        requested.add(v.key);
      }

      if (v.key.endsWith(':Error')) {
        const tag = v.key.split(':')[0];
        requested.add(`${tag}:PV`);
        requested.add(`${tag}:Target`);
      }

      if (v.key.endsWith(':ConstraintDistance')) {
        const tag = v.key.split(':')[0];
        requested.add(`${tag}:PV`);
        requested.add(`${tag}:LowLimit`);
        requested.add(`${tag}:HighLimit`);
      }

      if (v.key.endsWith(':SaturationFlag')) {
        const tag = v.key.split(':')[0];
        requested.add(`${tag}:OP`);
        requested.add(`${tag}:LowLimit`);
        requested.add(`${tag}:HighLimit`);
      }
    });

    enabledCvBases.forEach(cv => {
      requested.add(`${cv}:PV`);
      requested.add(`${cv}:Target`);
      requested.add(`${cv}:LowLimit`);
      requested.add(`${cv}:HighLimit`);
      requested.add(`${cv}:Prediction`);
    });

    enabledMvBases.forEach(mv => {
      requested.add(`${mv}:OP`);
      requested.add(`${mv}:SP`);
      requested.add(`${mv}:Mode`);
      requested.add(`${mv}:ModeTarget`);
      requested.add(`${mv}:LowLimit`);
      requested.add(`${mv}:HighLimit`);
    });

    const tagsParam = Array.from(requested).join(',');
    const startIso = new Date(requestStartMs).toISOString();
    const endIso = new Date(requestEndMs).toISOString();
    const query = new URLSearchParams({
      tags: tagsParam,
      start: startIso,
      end: endIso,
    });

    if (samplingMode === 'best_fit') {
      const activeWindowMs = Math.max(1, currentWindow.endMs - currentWindow.startMs);
      query.set('bucket_ms', String(getBestFitBucketMs(activeWindowMs)));
    }

    try {
      const response = await apiFetch(`/api/trends?${query.toString()}`);
      if (response.status === 401) {
        setAuthError(true);
        setErrorText('Session expired or insufficient permissions. Please log in again.');
        setTrendsData([]);
        setChartData([]);
        return;
      }
      if (!response.ok) throw new Error(`HTTP ${response.status}`);

      const data: TrendsDataPoint[] = await response.json();
      setTrendsData(data);

      const rawRows = enrichDerivedRows(buildRawRows(data));
      setFullData(rawRows);
      setBufferStartMs(requestStartMs);
      setBufferEndMs(requestEndMs);

      setVariables(prev => {
        let changed = false;
        const next = { ...prev };

        const windowRows = getRowsInWindow(rawRows, currentWindow.startMs, currentWindow.endMs);

        enabledVariables.forEach(variable => {
          if (!variable.autoScale) return;

          const values = getSeriesValues(windowRows, variable.key);
          if (values.length === 0) return;

          const min = Math.min(...values);
          const max = Math.max(...values);
          const padding = (max - min) * 0.08 || 1;
          const suggestedMin = Number((min - padding).toFixed(3));
          const suggestedMax = Number((max + padding).toFixed(3));

          if (next[variable.key].yMin !== suggestedMin || next[variable.key].yMax !== suggestedMax) {
            next[variable.key] = {
              ...next[variable.key],
              yMin: suggestedMin,
              yMax: suggestedMax,
            };
            changed = true;
          }
        });

        return changed ? next : prev;
      });
    } catch (e) {
      console.error('Failed to fetch trends:', e);
      setErrorText(e instanceof Error ? e.message : 'Failed to fetch trend data');
    } finally {
      if (!silent) {
        setLoading(false);
      }
    }
  }, [
    currentWindow.endMs,
    currentWindow.startMs,
    enabledVariables,
    getRowsInWindow,
    getSeriesValues,
    samplingMode,
  ]);

  const requestBufferedFetch = useCallback(
    (windowStartMs: number, windowEndMs: number, options?: { silent?: boolean }) => {
      const duration = getRangeDurationMs(timeRange);
      const preload = duration * getWindowPreloadFactor(timeRange);
      const requestStart = Math.max(0, windowStartMs - preload);
      const requestEnd = windowMode === 'relative'
        ? Math.min(Date.now(), windowEndMs + preload)
        : windowEndMs + preload;

      void fetchTrendsData(requestStart, requestEnd, options);
    },
    [fetchTrendsData, timeRange, windowMode]
  );

  const visibleWindowRows = useMemo(
    () => getRowsInWindow(fullData, currentWindow.startMs, currentWindow.endMs),
    [currentWindow.endMs, currentWindow.startMs, fullData, getRowsInWindow]
  );

  useEffect(() => {
    if (authError) return;

    if (!enabledKeySignature) {
      setTrendsData([]);
      setChartData([]);
      setFullData([]);
      setBufferStartMs(null);
      setBufferEndMs(null);
      return;
    }

    const needsFetch =
      bufferStartMs === null ||
      bufferEndMs === null ||
      currentWindow.startMs < bufferStartMs ||
      currentWindow.endMs > bufferEndMs;

    if (needsFetch) {
      requestBufferedFetch(currentWindow.startMs, currentWindow.endMs, {
        silent: chartData.length > 0,
      });
    }
  }, [
    authError,
    bufferEndMs,
    bufferStartMs,
    chartData.length,
    currentWindow.endMs,
    currentWindow.startMs,
    enabledKeySignature,
    requestBufferedFetch,
  ]);

  useEffect(() => {
    if (authError || !enabledKeySignature || !liveAutoRefresh) return;

    const interval = setInterval(() => {
      const now = Date.now();
      const duration = getRangeDurationMs(timeRange);
      setWindowEndMs(now);
      requestBufferedFetch(now - duration, now, { silent: true });
    }, 30000);

    return () => clearInterval(interval);
  }, [authError, enabledKeySignature, liveAutoRefresh, requestBufferedFetch, timeRange]);

  useEffect(() => {
    if (visibleWindowRows.length === 0) {
      setChartData([]);
      return;
    }

    setChartData(getDisplayRows(visibleWindowRows, enabledVariables));
  }, [enabledVariables, getDisplayRows, visibleWindowRows]);

  useEffect(() => {
    if (typeof window === 'undefined') return;

    const compactVars: Record<string, Partial<VariableConfig>> = {};
    Object.values(variables).forEach(v => {
      compactVars[v.key] = {
        enabled: v.enabled,
        autoScale: v.autoScale,
        axis: v.axis,
        alias: v.alias,
        unit: v.unit,
        pinned: v.pinned,
      };
    });

    const payload: SavedPrefs = {
      timeRange,
      maxPoints,
      showEvents,
      maxVisiblePerPane,
      showLegend,
      showPointMarkers,
      samplingMode,
      variables: compactVars,
    };
    window.localStorage.setItem(PREFS_KEY, JSON.stringify(payload));
  }, [variables, timeRange, maxPoints, showEvents, maxVisiblePerPane, samplingMode, showLegend, showPointMarkers]);

  const computePaneDomain = useCallback((pane: 'cv' | 'mv' | 'dv', side: 'left' | 'right'): [number | 'auto', number | 'auto'] => {
    const paneVars = enabledVariables.filter(v => v.pane === pane && v.axis === side);
    if (paneVars.length === 0) return ['auto', 'auto'];

    const mins: number[] = [];
    const maxs: number[] = [];

    paneVars.forEach(v => {
      if (!v.autoScale && v.yMin !== null && v.yMax !== null && v.yMin < v.yMax) {
        mins.push(v.yMin);
        maxs.push(v.yMax);
        return;
      }

      const values = getSeriesValues(chartData, v.key);
      if (values.length === 0) return;
      mins.push(Math.min(...values));
      maxs.push(Math.max(...values));
    });

    if (mins.length === 0 || maxs.length === 0) return ['auto', 'auto'];

    const min = Math.min(...mins);
    const max = Math.max(...maxs);
    const padding = (max - min) * 0.08 || 1;
    return [Number((min - padding).toFixed(3)), Number((max + padding).toFixed(3))];
  }, [chartData, enabledVariables, getSeriesValues]);

  const paneDomains = useMemo(() => ({
    cv: {
      left: computePaneDomain('cv', 'left'),
      right: computePaneDomain('cv', 'right'),
    },
    mv: {
      left: computePaneDomain('mv', 'left'),
      right: computePaneDomain('mv', 'right'),
    },
    dv: {
      left: computePaneDomain('dv', 'left'),
      right: computePaneDomain('dv', 'right'),
    },
  }), [computePaneDomain]);

  const modeEvents = useMemo<TrendEvent[]>(() => {
    if (!showEvents || visibleWindowRows.length < 2) return [];

    const events: TrendEvent[] = [];
    const keys = Object.keys(variables).filter(k => k.endsWith(':Mode') || k.endsWith(':ModeTarget'));
    const previous: Record<string, number> = {};

    visibleWindowRows.forEach(row => {
      keys.forEach(key => {
        const value = row[key];
        if (typeof value !== 'number') return;
        if (previous[key] === undefined) {
          previous[key] = value;
          return;
        }
        if (previous[key] !== value) {
          const base = key.split(':')[0];
          const field = key.split(':')[1];
          events.push({
            ts: row.ts,
            label: `${base} ${field} ${previous[key]} -> ${value}`,
            color: field === 'Mode' ? '#ef4444' : '#f59e0b',
          });
          previous[key] = value;
        }
      });
    });

    return events.slice(-40);
  }, [showEvents, variables, visibleWindowRows]);

  const activeMpcBands = useMemo<Array<{ x1: number; x2: number }>>(() => {
    const modeKeys = Object.keys(variables).filter(key => key.endsWith(':Mode') && variables[key].enabled);
    if (modeKeys.length === 0 || visibleWindowRows.length < 2) return [];

    const key = modeKeys[0];
    const segments: Array<{ x1: number; x2: number }> = [];
    let start: number | null = null;

    visibleWindowRows.forEach((row, index) => {
      const value = row[key];
      const isMpc = typeof value === 'number' && value === 3;

      if (isMpc && start === null) {
        start = row.ts;
      }

      if ((!isMpc || index === visibleWindowRows.length - 1) && start !== null) {
        const endTs = isMpc && index === visibleWindowRows.length - 1 ? row.ts : visibleWindowRows[Math.max(0, index - 1)].ts;
        segments.push({ x1: start, x2: endTs });
        start = null;
      }
    });

    return segments;
  }, [variables, visibleWindowRows]);

  const enabledCount = enabledVariables.length;

  return (
    <div className="space-y-4">
      <div className="bg-slate-900/50 rounded-xl border border-slate-800 p-4">
        <div className="mb-3 flex flex-wrap items-center gap-3 border-b border-slate-800 pb-3">
          <p className="text-[10px] uppercase font-bold text-slate-500">Time Range</p>
          <div className="flex gap-2">
            {(['15m', '1h', '4h', '12h', '24h'] as TimeRange[]).map(range => (
              <button
                key={range}
                onClick={() => {
                  setTimeRange(range);
                  if (liveAutoRefresh && windowMode === 'relative') {
                    setWindowEndMs(Date.now());
                  }
                }}
                className={`px-3 py-1.5 text-xs rounded-md transition-colors ${
                  timeRange === range ? 'bg-indigo-600 text-white' : 'bg-slate-800 text-slate-300 hover:bg-slate-700'
                }`}
              >
                {range}
              </button>
            ))}
          </div>

          <p className="ml-4 text-[10px] uppercase font-bold text-slate-500">Window</p>
          <div className="flex gap-2">
            <button
              onClick={() => shiftWindow(-1)}
              className="px-3 py-1.5 text-xs rounded-md bg-slate-800 text-slate-300 hover:bg-slate-700"
              title="Move backward by TimeRange / 10"
            >
              Back
            </button>
            <button
              onClick={() => shiftWindow(1)}
              className="px-3 py-1.5 text-xs rounded-md bg-slate-800 text-slate-300 hover:bg-slate-700"
              title="Move forward by TimeRange / 10"
            >
              Forward
            </button>
            <button
              onClick={useLiveWindow}
              className={`px-3 py-1.5 text-xs rounded-md transition-colors ${
                liveAutoRefresh
                  ? 'bg-emerald-700 text-white hover:bg-emerald-600'
                  : 'bg-slate-700 text-slate-400 ring-1 ring-slate-600'
              }`}
              aria-pressed={liveAutoRefresh}
            >
              Live
            </button>
          </div>

          {!liveAutoRefresh && (
            <span className="text-xs text-amber-300">Live auto-refresh paused</span>
          )}

          <span className="ml-auto text-xs text-slate-400">
            {enabledCount} selected | {chartData.length} shown | {visibleWindowRows.length} window pts | {fullData.length} buffered pts | {trendsData.length} records
          </span>
        </div>

        <div className="mb-3 flex flex-wrap items-end gap-3 border-b border-slate-800 pb-3">
          <label className="text-xs text-slate-300 flex flex-col gap-1">
            <span className="text-[10px] uppercase text-slate-500">Start</span>
            <input
              type="text"
              value={customStartInput}
              onChange={e => setCustomStartInput(e.target.value)}
              placeholder="YYYY-MM-DD HH:mm"
              className="bg-slate-950 text-slate-200 border border-slate-700 rounded px-2 py-1"
            />
          </label>

          <label className="text-xs text-slate-300 flex flex-col gap-1">
            <span className="text-[10px] uppercase text-slate-500">End</span>
            <input
              type="text"
              value={customEndInput}
              onChange={e => setCustomEndInput(e.target.value)}
              placeholder="YYYY-MM-DD HH:mm"
              className="bg-slate-950 text-slate-200 border border-slate-700 rounded px-2 py-1"
            />
          </label>

          <button
            onClick={applyCustomWindow}
            className={`px-3 py-1.5 text-xs rounded-md ${windowMode === 'custom' ? 'bg-emerald-700 text-white' : 'bg-slate-800 text-slate-300 hover:bg-slate-700'}`}
          >
            Apply
          </button>
        </div>

        <div className="mb-3 flex flex-wrap items-center gap-2">
          <p className="text-[10px] uppercase font-bold text-slate-500">Presets</p>
          {(['APC', 'CV', 'MV', 'DV', 'ALL'] as TrendPreset[]).map(preset => (
            <button
              key={preset}
              onClick={() => applyPreset(preset)}
              className="px-2.5 py-1 text-xs rounded-md bg-slate-800 text-slate-300 hover:bg-slate-700 transition-colors"
            >
              {preset}
            </button>
          ))}

          <div className="ml-4 flex items-center gap-2">
            <span className="text-[10px] uppercase text-slate-500">Max Points</span>
            <input
              type="number"
              min={200}
              max={5000}
              step={100}
              value={maxPoints}
              onChange={e => setMaxPoints(Math.max(200, Math.min(5000, Number(e.target.value) || 1200)))}
              className="w-24 bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs text-slate-200"
            />
          </div>

          <div className="ml-4 flex items-center gap-2">
            <span className="text-[10px] uppercase text-slate-500">Sampling</span>
            <select
              value={samplingMode}
              onChange={e => setSamplingMode(e.target.value as SamplingMode)}
              className="bg-slate-950 text-slate-200 border border-slate-700 rounded px-2 py-1 text-xs"
            >
              <option value="all">All values</option>
              <option value="best_fit">Best fit</option>
            </select>
          </div>

          <div className="ml-4 flex items-center gap-2">
            <span className="text-[10px] uppercase text-slate-500">Visible Lines/Pane</span>
            <input
              type="number"
              min={4}
              max={30}
              step={1}
              value={maxVisiblePerPane}
              onChange={e => setMaxVisiblePerPane(Math.max(4, Math.min(30, Number(e.target.value) || 10)))}
              className="w-20 bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs text-slate-200"
            />
          </div>

          <label className="flex items-center gap-2 text-xs text-slate-300">
            <input
              type="checkbox"
              checked={showEvents}
              onChange={e => setShowEvents(e.target.checked)}
              className="w-4 h-4 rounded border-slate-600 bg-slate-800"
            />
            Show events
          </label>

          <label className="flex items-center gap-2 text-xs text-slate-300">
            <input
              type="checkbox"
              checked={showLegend}
              onChange={e => setShowLegend(e.target.checked)}
              className="w-4 h-4 rounded border-slate-600 bg-slate-800"
            />
            Show legend
          </label>

          <label className="flex items-center gap-2 text-xs text-slate-300">
            <input
              type="checkbox"
              checked={showPointMarkers}
              onChange={e => setShowPointMarkers(e.target.checked)}
              className="w-4 h-4 rounded border-slate-600 bg-slate-800"
            />
            Show data point markers
          </label>
        </div>

        {errorText && (
          <div className="mb-3 rounded-lg border border-rose-700/60 bg-rose-950/40 p-2 text-xs text-rose-200">
            Trend query failed: {errorText}
          </div>
        )}

        {loading && chartData.length === 0 && (
          <div className="h-96 flex items-center justify-center text-slate-500">
            Loading trends data...
          </div>
        )}

        {enabledCount === 0 && (
          <div className="h-96 flex items-center justify-center text-slate-500">
            Select variables below to trend QuestDB history
          </div>
        )}

        {enabledCount > 0 && chartData.length === 0 && !loading && (
          <div className="h-96 flex items-center justify-center text-slate-500">
            No data available for selected time range
          </div>
        )}

        {chartData.length > 0 && (
          <TrendCharts
            chartData={chartData}
            loading={loading}
            showEvents={showEvents}
            showLegend={showLegend}
            showPointMarkers={showPointMarkers}
            modeEvents={modeEvents}
            activeMpcBands={activeMpcBands}
            visiblePaneVariables={visiblePaneVariables}
            paneVariables={paneVariables}
            paneDomains={paneDomains}
            variables={variables}
          />
        )}
      </div>

      <div className="bg-slate-900/50 rounded-xl border border-slate-800 p-3">
        <div className="flex items-center gap-2 flex-wrap">
          <p className="text-[10px] uppercase font-bold text-slate-500">Quick Select</p>
          <div className="flex gap-2">
            {(['ALL', 'NONE', 'CV', 'MV', 'DV'] as const).map(kind => (
              <button
                key={kind}
                onClick={() => setEnabledByType(kind)}
                className="px-2.5 py-1 text-xs rounded-md bg-slate-800 text-slate-300 hover:bg-slate-700 transition-colors"
              >
                {kind}
              </button>
            ))}
          </div>
          <button
            onClick={() => requestBufferedFetch(currentWindow.startMs, currentWindow.endMs)}
            disabled={enabledCount === 0 || loading}
            className="ml-auto px-3 py-1.5 text-xs rounded-md bg-cyan-700 text-white hover:bg-cyan-600 disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Refresh now
          </button>
        </div>
      </div>

      <SeriesConfigTable
        seriesRows={seriesRows}
        seriesViewMode={seriesViewMode}
        seriesTypeFilter={seriesTypeFilter}
        seriesSearch={seriesSearch}
        onSetSeriesViewMode={setSeriesViewMode}
        onSetSeriesTypeFilter={setSeriesTypeFilter}
        onSetSeriesSearch={setSeriesSearch}
        onToggleVariable={toggleVariable}
        onUpdateVariableConfig={updateVariableConfig}
      />
    </div>
  );
};
