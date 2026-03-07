import React, { useState, useEffect, useCallback } from 'react';
import { apiFetch } from '../lib/api';

interface ControllerSummary {
  id: string;
  models: string[];
  active_model?: string;
  state: any;
  last_error?: string;
  started_at?: string;
  stopped_at?: string;
  last_log_path?: string;
}

interface LogTailResponse {
  id: string;
  log_path?: string;
  lines: string[];
  error?: string;
}

const parseState = (state: any): { status: 'Running' | 'Stopped' | 'Failed'; pid?: number } => {
  if (state === 'Stopped') return { status: 'Stopped' };
  if (state === 'Failed') return { status: 'Failed' };
  if (typeof state === 'object' && state?.Running !== undefined) return { status: 'Running', pid: state.Running };
  return { status: 'Stopped' };
};

const formatRelative = (iso?: string): string => {
  if (!iso) return '—';
  const d = new Date(iso);
  const diff = Math.floor((Date.now() - d.getTime()) / 1000);
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return d.toLocaleString();
};

const stateColor = (status: 'Running' | 'Stopped' | 'Failed') => {
  if (status === 'Running') return 'text-emerald-400';
  if (status === 'Failed') return 'text-red-400';
  return 'text-slate-400';
};

const stateDot = (status: 'Running' | 'Stopped' | 'Failed') => {
  if (status === 'Running') return 'bg-emerald-400 animate-pulse';
  if (status === 'Failed') return 'bg-red-500';
  return 'bg-slate-500';
};

interface Props {
  controllerId: string;
  summary?: ControllerSummary;
}

export const ControllerDiagnostics: React.FC<Props> = ({ controllerId, summary }) => {
  const [log, setLog] = useState<LogTailResponse | null>(null);
  const [logLoading, setLogLoading] = useState(false);
  const [logError, setLogError] = useState<string | null>(null);
  const [autoRefresh, setAutoRefresh] = useState(true);

  const fetchLog = useCallback(async () => {
    if (!controllerId) return;
    setLogLoading(true);
    try {
      const res = await apiFetch(`/api/prox/controllers/${controllerId}/logs/tail?lines=80`);
      const data: LogTailResponse = await res.json();
      setLog(data);
      setLogError(null);
    } catch (e: any) {
      setLogError(e.message ?? 'Failed to fetch log');
    } finally {
      setLogLoading(false);
    }
  }, [controllerId]);

  useEffect(() => {
    fetchLog();
  }, [fetchLog]);

  useEffect(() => {
    if (!autoRefresh) return;
    const interval = setInterval(fetchLog, 5000);
    return () => clearInterval(interval);
  }, [autoRefresh, fetchLog]);

  if (!summary) {
    return (
      <div className="flex items-center justify-center h-full text-slate-500 text-sm">
        No controller selected.
      </div>
    );
  }

  const { status, pid } = parseState(summary.state);

  return (
    <div className="flex flex-col gap-4 p-4 h-full overflow-y-auto">
      {/* Status card */}
      <div className="bg-slate-800 border border-slate-700 rounded-lg p-4 grid grid-cols-2 md:grid-cols-4 gap-4">
        <div>
          <p className="text-xs text-slate-500 mb-1">Controller</p>
          <p className="text-sm font-semibold text-white">{summary.id}</p>
        </div>
        <div>
          <p className="text-xs text-slate-500 mb-1">State</p>
          <div className="flex items-center gap-2">
            <span className={`inline-block w-2 h-2 rounded-full ${stateDot(status)}`} />
            <span className={`text-sm font-semibold ${stateColor(status)}`}>
              {status}{pid ? ` (PID ${pid})` : ''}
            </span>
          </div>
        </div>
        <div>
          <p className="text-xs text-slate-500 mb-1">Active Model</p>
          <p className="text-sm text-slate-300">{summary.active_model ?? '—'}</p>
        </div>
        <div>
          <p className="text-xs text-slate-500 mb-1">Started</p>
          <p className="text-sm text-slate-300">{formatRelative(summary.started_at)}</p>
        </div>
      </div>

      {/* Error card — only shown when there is an error */}
      {(summary.last_error || status === 'Failed') && (
        <div className="bg-red-950 border border-red-800 rounded-lg p-4">
          <p className="text-xs text-red-400 font-semibold mb-1 uppercase tracking-wide">Last Error</p>
          <p className="text-sm text-red-300 font-mono">{summary.last_error ?? 'Engine exited with non-zero status'}</p>
          {summary.stopped_at && (
            <p className="text-xs text-red-500 mt-1">{formatRelative(summary.stopped_at)}</p>
          )}
        </div>
      )}

      {/* Log tail */}
      <div className="flex-1 bg-slate-900 border border-slate-700 rounded-lg flex flex-col min-h-0">
        <div className="flex items-center justify-between px-4 py-2 border-b border-slate-700">
          <div>
            <span className="text-xs font-semibold text-slate-300">Engine Log</span>
            {log?.log_path && (
              <span className="ml-2 text-xs text-slate-600">{log.log_path}</span>
            )}
          </div>
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-1.5 text-xs text-slate-400 cursor-pointer">
              <input
                type="checkbox"
                checked={autoRefresh}
                onChange={e => setAutoRefresh(e.target.checked)}
                className="accent-emerald-500"
              />
              Auto-refresh
            </label>
            <button
              onClick={fetchLog}
              disabled={logLoading}
              className="text-xs px-2 py-1 rounded bg-slate-700 hover:bg-slate-600 text-slate-300 disabled:opacity-40"
            >
              {logLoading ? 'Loading…' : 'Refresh'}
            </button>
          </div>
        </div>
        <div className="flex-1 overflow-y-auto p-3 font-mono text-xs text-slate-300 leading-relaxed">
          {logError && (
            <p className="text-red-400">{logError}</p>
          )}
          {!logError && (!log || log.lines.length === 0) && (
            <p className="text-slate-600 italic">No log output available.</p>
          )}
          {log?.lines.map((line, i) => {
            // Colour-code lines by severity markers
            const isError = /❌|🔥|🔴|CRITICAL|ERROR|\bfailed\b/.test(line);
            const isWarn  = /⚠️|🚨|WARNING|warn/i.test(line);
            const isGood  = /✅|recovered|Solved|connected/i.test(line);
            const cls = isError ? 'text-red-400' : isWarn ? 'text-amber-400' : isGood ? 'text-emerald-400' : 'text-slate-400';
            return (
              <div key={i} className={cls}>{line}</div>
            );
          })}
        </div>
      </div>
    </div>
  );
};
