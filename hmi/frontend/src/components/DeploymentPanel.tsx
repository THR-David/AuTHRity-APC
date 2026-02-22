import React, { useState, useEffect } from 'react';
import { Play, Square, RefreshCw, Server, AlertCircle, Folder } from 'lucide-react';
import { apiFetch } from '../lib/api';

interface ControllerInfo {
  id: string; // Folder Name
  models: string[]; // JSON filenames
  active_model?: string; // Currently running model (filename)
  state: any; // Raw state
}

// Helper to parse the Rust Enum JSON serialization
const parseState = (state: any): { status: 'Running' | 'Stopped' | 'Failed', pid?: number } => {
    if (state === 'Stopped') return { status: 'Stopped' };
    if (state === 'Failed') return { status: 'Failed' };
    if (typeof state === 'object' && state.Running !== undefined) {
        return { status: 'Running', pid: state.Running };
    }
    return { status: 'Stopped' }; // Default/Fallback
};

interface DeploymentPanelProps {
  filterId?: string;
  onSaveConfig?: () => void;
  isSaving?: boolean;
}

export const DeploymentPanel: React.FC<DeploymentPanelProps> = ({ filterId, onSaveConfig, isSaving }) => {
    const [controllers, setControllers] = useState<ControllerInfo[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [actionId, setActionId] = useState<string | null>(null);
    
    // Map of ControllerID -> Selected Model Filename
    const [selections, setSelections] = useState<Record<string, string>>({});

    const fetchControllers = async () => {
        if (controllers.length === 0) setLoading(true);
        try {
            const res = await apiFetch('/api/prox/controllers');
            if (!res.ok) throw new Error(await res.text());
            const data: ControllerInfo[] = await res.json();
            
            // Filter if props provided
            const filteredData = filterId 
              ? data.filter(c => c.id === filterId)
              : data;

            setControllers(filteredData);
            
            // Initial selection logic: default to first model or active model
            setSelections(prev => {
                const newSel = { ...prev };
                filteredData.forEach(c => {
                    // Only set default if not already set by user
                    if (!newSel[c.id]) {
                         if (c.active_model) {
                             newSel[c.id] = c.active_model;
                         } else if (c.models.length > 0) {
                             newSel[c.id] = c.models[0];
                         }
                    }
                });
                return newSel;
            });

            setError(null);
        } catch (err: any) {
            setError(err.message);
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        fetchControllers();
        const interval = setInterval(fetchControllers, 2000); 
        return () => clearInterval(interval);
    }, []);

    const handleStart = async (controllerId: string) => {
        const model = selections[controllerId];
        if (!model) return;
        
        setActionId(controllerId);
        try {
            await apiFetch(`/api/prox/controllers/${controllerId}/start`, { 
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ model_filename: model }) 
            }, true);
            await fetchControllers();
        } catch (err) {
            console.error(err);
        } finally {
            setActionId(null);
        }
    };

    const handleStop = async (controllerId: string) => {
        setActionId(controllerId);
        try {
            await apiFetch(`/api/prox/controllers/${controllerId}/stop`, { method: 'POST' }, true);
            await fetchControllers();
        } catch (err) {
            console.error(err);
        } finally {
            setActionId(null);
        }
    };

    const anyRunning = controllers.some(c => parseState(c.state).status === 'Running');

    return (
        <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg mb-6">
            <div className={`px-4 py-3 border-b-2 flex justify-between items-center transition-colors ${anyRunning ? 'bg-gradient-to-r from-emerald-900/40 to-emerald-800/40 border-emerald-500/30' : 'bg-gradient-to-r from-blue-900/20 to-blue-800/20 border-slate-700'}`}>
                <h3 className="font-extrabold text-slate-100 text-[13px] tracking-wide uppercase flex items-center gap-2">
                    <Server size={16} className={anyRunning ? "text-emerald-400" : "text-blue-400"} />
                    APC Engine Manager
                </h3>
                <div className="flex items-center gap-3">
                    {onSaveConfig && (
                        <button 
                            onClick={onSaveConfig}
                            disabled={isSaving}
                            className="px-3 py-1 bg-emerald-600 hover:bg-emerald-500 disabled:bg-slate-600 rounded text-xs font-bold text-white transition flex items-center gap-2 disabled:cursor-not-allowed"
                            title="Save current tuning parameters to model file"
                        >
                            💾 {isSaving ? 'Saving...' : 'Save Config'}
                        </button>
                    )}
                    <button onClick={fetchControllers} className="text-slate-400 hover:text-white transition-colors">
                        <RefreshCw size={14} className={loading && controllers.length === 0 ? "animate-spin" : ""} />
                    </button>
                </div>
            </div>

            <div className="p-4 grid gap-4">
                {error && (
                    <div className="bg-red-900/20 border border-red-500/30 text-red-400 p-3 rounded text-xs flex items-center gap-2">
                        <AlertCircle size={14} />
                        {error}
                    </div>
                )}
                
                {controllers.map(c => {
                    const { status, pid } = parseState(c.state);
                    const isRunning = status === 'Running';
                    
                    return (
                        <div key={c.id} className={`border rounded-lg p-4 transition-all ${isRunning ? 'bg-slate-800/60 border-emerald-500/30' : 'bg-slate-800/30 border-slate-700'}`}>
                            <div className="flex items-start justify-between mb-4">
                                <div className="flex items-center gap-3">
                                    <div className={`p-2 rounded-lg ${isRunning ? 'bg-emerald-500/10 text-emerald-400' : 'bg-blue-500/10 text-blue-400'}`}>
                                        <Folder size={20} />
                                    </div>
                                    <div>
                                        <h4 className="font-bold text-slate-200 text-sm">{c.id}</h4>
                                        <div className="flex items-center gap-2 mt-1">
                                            <div className={`w-1.5 h-1.5 rounded-full ${isRunning ? 'bg-emerald-500 animate-pulse' : 'bg-slate-600'}`} />
                                            <span className="text-[10px] font-mono text-slate-400 uppercase">
                                                {isRunning ? `Running (PID: ${pid})` : 'Stopped'}
                                            </span>
                                        </div>
                                    </div>
                                </div>
                                <div className="text-right">
                                    {isRunning && c.active_model && (
                                        <div className="text-[10px] font-mono bg-emerald-900/40 text-emerald-400 px-2 py-1 rounded border border-emerald-500/20">
                                            Active: {c.active_model}
                                        </div>
                                    )}
                                </div>
                            </div>

                            <div className="flex gap-3">
                                <div className="flex-1">
                                    <select 
                                        value={selections[c.id] || ""}
                                        onChange={(e) => setSelections(prev => ({...prev, [c.id]: e.target.value}))}
                                        disabled={isRunning || c.models.length === 0}
                                        className={`w-full text-xs font-mono rounded px-3 py-2 border bg-slate-900 focus:outline-none focus:ring-1 ${isRunning ? 'border-slate-700 text-slate-500 cursor-not-allowed' : 'border-slate-600 text-slate-300 focus:border-blue-500'}`}
                                    >
                                        {c.models.length === 0 && <option>No models found</option>}
                                        {c.models.map(m => (
                                            <option key={m} value={m}>{m}</option>
                                        ))}
                                    </select>
                                </div>
                                <button
                                    onClick={() => isRunning ? handleStop(c.id) : handleStart(c.id)}
                                    disabled={actionId === c.id || (!isRunning && !selections[c.id])}
                                    className={`px-4 rounded text-xs font-bold transition-all flex items-center gap-2 min-w-[100px] justify-center ${
                                        isRunning 
                                        ? 'bg-rose-900/30 text-rose-400 border border-rose-800 hover:bg-rose-900/50' 
                                        : 'bg-emerald-900/30 text-emerald-400 border border-emerald-800 hover:bg-emerald-900/50'
                                    } disabled:opacity-50 disabled:cursor-not-allowed`}
                                >
                                    {actionId === c.id ? <RefreshCw size={14} className="animate-spin"/> : isRunning ? <Square size={14} fill="currentColor"/> : <Play size={14} fill="currentColor"/>}
                                    {isRunning ? 'STOP' : 'START'}
                                </button>
                            </div>
                        </div>
                    );
                })}

                {controllers.length === 0 && !loading && (
                    <div className="text-center py-8 text-slate-500 text-sm italic">
                        No controllers found in ./models
                    </div>
                )}
            </div>
        </div>
    );
};
