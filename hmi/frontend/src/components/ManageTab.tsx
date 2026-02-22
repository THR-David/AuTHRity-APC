import React, { useState, useEffect } from 'react';
import { useTagStore } from '../store/tagStore';
import { useWriteNode } from '../hooks/useWriteNode';
import { DeploymentPanel } from './DeploymentPanel';

interface ManageTabProps {
  systemPrefix: string;
  wsRef: React.RefObject<WebSocket | null>;
  cvList: string[];
  mvList: string[];
  dvList: string[];
}

export const ManageTab: React.FC<ManageTabProps> = ({ 
  systemPrefix,
  wsRef
}) => {
  const tags = useTagStore(s => s.tags);
  const [modelSampleTime, setModelSampleTime] = useState<number>(0);
  
  const { isWriting: isSaving, write: writeSaveConfig } = useWriteNode(
    `${systemPrefix}:SaveConfiguration`,
    wsRef,
    {
      onSuccess: () => console.log('✅ Configuration saved successfully')
    }
  );
  
  // Fetch model to get sample time
  useEffect(() => {
    const fetchModel = async () => {
      try {
        const res = await fetch('/api/controller/model');
        if (res.ok) {
          const data = await res.json();
          if (data.sample_time !== undefined) {
            setModelSampleTime(data.sample_time);
          }
        }
      } catch (err) {
        console.error('Failed to fetch model:', err);
      }
    };
    fetchModel();
  }, []);
  
  const handleSaveConfiguration = () => {
    if (!isSaving) {
      writeSaveConfig(true);
      console.log("💾 Save configuration triggered");
    }
  };

  // Helper to format values
  const formatValue = (value: any): string => {
    if (value === undefined || value === null) return "--";
    if (typeof value === "number") return value.toFixed(2);
    return String(value);
  };

  // Helper to format timestamp
  const formatTimestamp = (timestamp: string | undefined): string => {
    if (!timestamp) return "--";
    try {
      return new Date(timestamp).toLocaleString();
    } catch {
      return timestamp;
    }
  };

  // Helper to safely get numeric value from tag
  const getNumericValue = (tagValue: any): number => {
    if (typeof tagValue === 'number') return tagValue;
    if (typeof tagValue === 'string') return parseFloat(tagValue) || 0;
    return 0;
  };

  // Check if execution time is concerning (warn if > 50% of sample time)
  // ExecutionTimeMs is in milliseconds, SampleTime is in seconds (from model)
  const executionTimeMs = getNumericValue(tags[`${systemPrefix}:ExecutionTimeMs`]?.value);
  const sampleTimeS = modelSampleTime; // Use sample time from model
  const thresholdMs = sampleTimeS * 1000 * 0.5; // Convert seconds to ms, then 50%
  const isExecutionTimeConcerning = sampleTimeS > 0 && executionTimeMs > thresholdMs;

  return (
    <div className="space-y-4 animate-in fade-in duration-300">
      
      {/* OVERALL STATUS BANNER */}
      <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
        <div className="p-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <div className="text-4xl text-slate-300">
                {tags[`${systemPrefix}:SolverStatus`]?.value === 1 ? '✓' : tags[`${systemPrefix}:SolverStatus`]?.value === 0 ? '◐' : '○'}
              </div>
              <div>
                <div className="text-xs uppercase tracking-wider text-slate-400 font-bold">Overall Controller Status</div>
                <div className="text-2xl font-bold text-slate-100">
                  {tags[`${systemPrefix}:SolverStatus`]?.value === 1 ? 'Optimal' : tags[`${systemPrefix}:SolverStatus`]?.value === 0 ? 'Solved' : 'Unknown'}
                </div>
              </div>
            </div>
            <div className="flex items-center gap-6 text-sm">
              <div className="text-center">
                <div className="text-xs text-slate-400 uppercase">Execution</div>
                <div className={`text-xl font-bold font-mono ${
                  isExecutionTimeConcerning
                    ? 'text-red-400' 
                    : 'text-emerald-400'
                }`}>
                  {formatValue(tags[`${systemPrefix}:ExecutionTimeMs`]?.value)} ms
                </div>
              </div>
              <div className="text-center">
                <div className="text-xs text-slate-400 uppercase">Objective</div>
                <div className="text-xl font-bold font-mono text-blue-400">
                  {formatValue(tags[`${systemPrefix}:ObjectiveFunction`]?.value)}
                </div>
              </div>
              <div className="text-center">
                <div className="text-xs text-slate-400 uppercase">Mode</div>
                <div className="text-xl font-bold">
                  {tags[`${systemPrefix}:OperatingMode`]?.value === 2 ? (
                    <span className="text-emerald-400">Control</span>
                  ) : tags[`${systemPrefix}:OperatingMode`]?.value === 1 ? (
                    <span className="text-blue-400">Calc</span>
                  ) : (
                    <span className="text-slate-500">Off</span>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* TWO COLUMN LAYOUT */}
      <div className="grid grid-cols-2 gap-4">
        
        {/* LEFT COLUMN: MODEL DEPLOYMENT */}
        <div className="space-y-4">
          <DeploymentPanel 
            filterId={systemPrefix} 
            onSaveConfig={handleSaveConfiguration}
            isSaving={isSaving}
          />
        </div>

        {/* RIGHT COLUMN: HEALTH METRICS */}
        <div className="space-y-4">
          
          {/* HEALTH METRICS */}
          <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
            <div className="bg-slate-800/80 px-4 py-2 border-b-2 border-slate-700">
              <h3 className="font-extrabold text-slate-100 text-xs tracking-wide uppercase">
                ❤️ Controller Health
              </h3>
            </div>

            <div className="p-4 space-y-3">
              {/* Solver Status - Prominent */}
              <div className="p-4 bg-slate-800/60 rounded-lg border-2 border-slate-700">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-[10px] font-bold text-slate-400 uppercase mb-1">Solver Status</div>
                    <div className="text-xl font-bold">
                      {tags[`${systemPrefix}:SolverStatus`]?.value === 1 ? (
                        <span className="text-emerald-400">✓ Optimal</span>
                      ) : tags[`${systemPrefix}:SolverStatus`]?.value === 0 ? (
                        <span className="text-blue-400">✓ Solved</span>
                      ) : (
                        <span className="text-slate-400">? Unknown</span>
                      )}
                    </div>
                  </div>
                  <div className="text-5xl opacity-20">
                    {tags[`${systemPrefix}:SolverStatus`]?.value === 1 ? '✓' : '◐'}
                  </div>
                </div>
              </div>

              {/* Metrics Grid */}
              <div className="grid grid-cols-2 gap-2">
                <div className="p-3 bg-slate-800/50 rounded border border-slate-700">
                  <div className="text-[9px] font-bold text-slate-400 uppercase mb-1">Execution Time</div>
                  <div className={`text-lg font-bold font-mono ${
                    isExecutionTimeConcerning
                      ? 'text-red-400'
                      : 'text-emerald-400'
                  }`}>
                    {formatValue(tags[`${systemPrefix}:ExecutionTimeMs`]?.value)} ms
                  </div>
                  {isExecutionTimeConcerning && (
                    <div className="text-[8px] text-red-400 mt-1">⚠️ High load!</div>
                  )}
                </div>

                <div className="p-3 bg-slate-800/50 rounded border border-slate-700/50">
                  <div className="text-[9px] font-bold text-slate-400 uppercase mb-1">Objective</div>
                  <div className="text-lg font-bold font-mono text-indigo-400">
                    {formatValue(tags[`${systemPrefix}:ObjectiveFunction`]?.value)}
                  </div>
                </div>

                <div className="p-3 bg-slate-800/50 rounded border border-slate-700/50 col-span-2">
                  <div className="text-[9px] font-bold text-slate-400 uppercase mb-1">Last Heartbeat</div>
                  <div className="text-[10px] font-mono text-slate-300">
                    {formatTimestamp(tags[`${systemPrefix}:Heartbeat`]?.timestamp)}
                  </div>
                </div>
              </div>
            </div>
          </div>

        </div>
      </div>

    </div>
  );
};
