import React, { useState, useEffect } from 'react';
import { useTagStore } from '../store/tagStore';
import { useWriteNode } from '../hooks/useWriteNode';
import { DeploymentPanel } from './DeploymentPanel';

interface ConfigurationTabProps {
  systemPrefix: string;
  wsRef: React.RefObject<WebSocket | null>;
}

export const ConfigurationTab: React.FC<ConfigurationTabProps> = ({ 
  systemPrefix,
  wsRef
}) => {
  const tags = useTagStore(s => s.tags);
  const [configStatus, setConfigStatus] = useState<string>("Ready");
  
  const { isWriting: isSaving, write: writeSaveConfig } = useWriteNode(
    `${systemPrefix}:SaveConfiguration`,
    wsRef,
    {
      onSuccess: () => console.log('✅ Configuration saved successfully')
    }
  );
  
  // Read configuration status from OPC UA
  useEffect(() => {
    const configStatusNode = `${systemPrefix}:ConfigurationStatus`;
    
    // Get configuration status
    const statusValue = tags[configStatusNode]?.value;
    if (typeof statusValue === 'string') {
      setConfigStatus(statusValue);
    }
  }, [tags, systemPrefix]);
  
  const handleSaveConfiguration = () => {
    if (!isSaving) {
      writeSaveConfig(true);
      console.log("💾 Save configuration triggered");
    }
  };

  return (
    <div className="space-y-6 animate-in fade-in duration-300">
      
      {/* RUNTIME CONFIGURATION & PERSISTENCE */}
      <div className="bg-slate-900/50 rounded-xl border-2 border-emerald-500/10 overflow-hidden shadow-lg p-4 flex justify-between items-center">
        <div>
           <div className="font-bold text-xs text-emerald-400 uppercase tracking-wider mb-1">Configuration Persistence</div>
           <div className="text-[10px] text-slate-400">Status: {configStatus === "Saving..." ? "Saving..." : "Running"}</div>
           <div className="text-[10px] text-slate-500 mt-1">Exports the current live tuning values (Weights, Constraints) to a new JSON file on the Controller Host.</div>
        </div>
        
        <button
           onClick={handleSaveConfiguration}
           disabled={isSaving}
           className="bg-emerald-600 hover:bg-emerald-500 text-white font-bold py-2 px-4 rounded shadow-lg text-xs transition-colors disabled:opacity-50"
        >
          {isSaving ? "Saving..." : "Save Config to Disk"}
        </button>
      </div>

      {/* ATOMIC DEPLOYMENT */}
      <DeploymentPanel filterId={systemPrefix} />

      {/* GLOBAL CONTROLLER PARAMETERS */}
      <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
        <div className="bg-gradient-to-r from-indigo-900 to-indigo-800 px-4 py-2 border-b-2 border-slate-700">
          <h3 className="font-extrabold text-slate-100 text-[13px] tracking-wide uppercase">
            ⚙️ Global Controller Parameters
          </h3>
        </div>

        <div className="p-4">
          <div className="grid grid-cols-2 gap-6">
            {/* Prediction Horizon */}
            <div>
              <label className="block text-[11px] font-bold text-indigo-400 uppercase mb-2">
                Prediction Horizon (P)
              </label>
              <input
                type="number"
                step="1"
                placeholder="45"
                className="w-full bg-slate-800 border border-indigo-600/50 rounded px-3 py-2 text-sm text-indigo-300 font-bold text-center hover:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 transition-all"
                disabled
                title="Not yet implemented - stored in model configuration"
              />
              <p className="text-[10px] text-slate-500 mt-1 text-center">Steps (read-only)</p>
            </div>

            {/* Sample Time */}
            <div>
              <label className="block text-[11px] font-bold text-indigo-400 uppercase mb-2">
                Sample Time (Ts)
              </label>
              <input
                type="number"
                step="1"
                placeholder="20"
                className="w-full bg-slate-800 border border-indigo-600/50 rounded px-3 py-2 text-sm text-indigo-300 font-bold text-center hover:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 transition-all"
                disabled
                title="Not yet implemented - stored in model configuration"
              />
              <p className="text-[10px] text-slate-500 mt-1 text-center">Seconds (read-only)</p>
            </div>
          </div>

          <div className="mt-4 p-3 bg-slate-800/50 rounded border border-slate-700/50">
            <p className="text-[11px] text-slate-400 leading-relaxed">
              <span className="font-bold text-slate-300">💡 Engineering Note:</span> Prediction Horizon (P) and Sample Time (Ts) are stored in the DMC model configuration file and require model reload to change. 
              Control Horizon (M) can be adjusted in the <span className="font-bold text-indigo-300">Tuning</span> tab.
            </p>
          </div>
        </div>
      </div>

      {/* SYSTEM INFORMATION */}
      <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
        <div className="bg-gradient-to-r from-slate-900 to-slate-800 px-4 py-2 border-b-2 border-slate-700">
          <h3 className="font-extrabold text-slate-100 text-[13px] tracking-wide uppercase">
            📊 System Information
          </h3>
        </div>

        <div className="p-4">
          <div className="grid grid-cols-2 gap-4">
            <div className="p-3 bg-slate-800/50 rounded border border-slate-700/50">
              <div className="text-[10px] font-bold text-slate-500 uppercase mb-1">Operating Mode</div>
              <div className="text-sm font-semibold text-emerald-400">
                {tags[`${systemPrefix}:OperatingMode`]?.value === 2 ? 'Control' :
                 tags[`${systemPrefix}:OperatingMode`]?.value === 1 ? 'Calculator' : 'Off'}
              </div>
            </div>
            
            <div className="p-3 bg-slate-800/50 rounded border border-slate-700/50">
              <div className="text-[10px] font-bold text-slate-500 uppercase mb-1">Solver Status</div>
              <div className="text-sm font-semibold text-amber-400">
                {tags[`${systemPrefix}:SolverStatus`]?.value === 1 ? 'Optimal' :
                 tags[`${systemPrefix}:SolverStatus`]?.value === 0 ? 'Solved' : 'Unknown'}
              </div>
            </div>
          </div>
        </div>
      </div>

    </div>
  );
};
