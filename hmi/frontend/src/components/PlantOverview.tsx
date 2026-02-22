import React, { useEffect, useState } from 'react';
import { useTagStore } from '../store/tagStore';
import { ToggleSwitch } from './ToggleSwitch';
import { NextRunDisplay } from './NextRunDisplay';

interface ModelInfo {
  name: string;
  prefix: string;
}

interface PlantOverviewProps {
  wsRef: React.RefObject<WebSocket | null>;
  onSelectModel: (name: string) => void;
}

export const PlantOverview: React.FC<PlantOverviewProps> = ({ wsRef, onSelectModel }) => {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [loading, setLoading] = useState(true);

  // 1. Fetch Models and Determine Prefixes
  useEffect(() => {
    const fetchAllModels = async () => {
      try {
        const res = await fetch('/api/models');
        const modelNames: string[] = await res.json();
        
        const infoPromises = modelNames.map(async (name) => {
           try {
             const configRes = await fetch(`/api/model?file=${name}`);
             const configData = await configRes.json();
             // Logic to find prefix (same as App.tsx)
             const systemNode = configData.nodes.find((n: any) => n.node_id.endsWith(":ControlNodes"));
             const prefix = systemNode ? systemNode.node_id.replace(":ControlNodes", "") : "System";
             return { name, prefix };
           } catch (e) {
             console.error(`Failed to load config for ${name}`, e);
             return { name, prefix: "" };
           }
        });

        const loadedModels = await Promise.all(infoPromises);
        setModels(loadedModels.filter(m => m.prefix !== ""));
        setLoading(false);
      } catch (err) {
        console.error("Failed to load plant overview", err);
        setLoading(false);
      }
    };

    fetchAllModels();
  }, []);

  const handleWrite = (nodeId: string, value: number) => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) return;
    wsRef.current.send(JSON.stringify({ type: "WRITE", nodeId, value }));
  };

  const handleToggle = (prefix: string, currentMode: number, isControlToggle: boolean, isCalcToggle: boolean) => {
      // Determine new mode based on toggle clicked
      // Mode 0: Idle
      // Mode 1: Minitor (Calc On, Control Off)
      // Mode 2: Engage (Calc On, Control On)
      
      let newMode = currentMode;
      const calcOn = currentMode >= 1;
      const controlOn = currentMode === 2;

      if (isCalcToggle) {
          // User clicked "Calculator"
          // If turning ON: Go to Monitor (1)
          // If turning OFF: Go to Idle (0)
          if (!calcOn) newMode = 1; // 0 -> 1
          else newMode = 0;         // 1/2 -> 0
      } else if (isControlToggle) {
           // User clicked "Control Action"
           // If turning ON: Go to Engage (2) (Must have calc on implicit)
           // If turning OFF: Go to Monitor (1)
           if (!controlOn) newMode = 2; // 1 -> 2
           else newMode = 1;            // 2 -> 1
      }

      handleWrite(`${prefix}:OperatingMode`, newMode);
  };

  if (loading) return <div className="p-8 text-slate-500">Scanning Plant Network...</div>;

  return (
    <div className="p-8 animate-in fade-in duration-500">
      <h2 className="text-2xl font-bold text-slate-100 mb-6">Plant Overview</h2>
      
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-6">
        {models.map((model) => (
          <ModelCard 
            key={model.name} 
            model={model} 
            onToggle={handleToggle}
            onSelect={() => onSelectModel(model.name)}
          />
        ))}
      </div>
    </div>
  );
};

// --- SUB-COMPONENT FOR INDIVIDUAL CARD ---
const ModelCard = ({ model, onToggle, onSelect }: { 
    model: ModelInfo, 
    onToggle: (p: string, m: number, c: boolean, k: boolean) => void,
    onSelect: () => void
}) => {
    // Subscribe to specific mode tag
    const opModeVal = useTagStore(state => state.tags[`${model.prefix}:OperatingMode`]?.value);
    const mode = typeof opModeVal === 'number' ? opModeVal : 0;
    
    const calcOn = mode >= 1;
    const controlOn = mode === 2;

    return (
        <div className="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-lg hover:border-indigo-500/30 transition-colors">
            <div className="flex justify-between items-start mb-4">
                <div>
                    <h3 className="text-lg font-bold text-white cursor-pointer hover:text-indigo-400" onClick={onSelect}>
                        {model.name}
                    </h3>
                    <div className="text-xs text-slate-500 font-mono mt-1">{model.prefix}</div>
                </div>
                {/* Next Run Display */}
                <div className="bg-slate-950 rounded-lg p-2 border border-slate-800">
                     <NextRunDisplay systemPrefix={model.prefix} />
                </div>
            </div>

            <div className="h-px bg-slate-800 my-4"></div>

            <div className="flex items-center justify-between gap-4">
                {/* Calculator Toggle */}
                <div className="flex flex-col items-center">
                    <ToggleSwitch 
                        label="Calculator"
                        enabled={calcOn}
                        onToggle={() => onToggle(model.prefix, mode, false, true)}
                        colorOn="bg-blue-500"
                    />
                    <span className={`text-[10px] mt-2 font-bold uppercase tracking-wider ${calcOn ? 'text-blue-400' : 'text-slate-600'}`}>
                        {calcOn ? "Running" : "Idle"}
                    </span>
                </div>

                {/* Status Indicator */}
                <div className="flex-1 flex flex-col items-center justify-center">
                    <div className={`w-3 h-3 rounded-full mb-1 ${
                        mode === 2 ? "bg-emerald-500 shadow-[0_0_10px_rgba(16,185,129,0.5)]" : 
                        mode === 1 ? "bg-blue-500 shadow-[0_0_10px_rgba(59,130,246,0.5)]" : 
                        "bg-slate-700"
                    }`}></div>
                    <span className="text-[9px] text-slate-500 font-mono">
                         {mode === 2 ? "ENGAGED" : mode === 1 ? "MONITOR" : "OFFLINE"}
                    </span>
                </div>

                {/* Control Toggle */}
                <div className="flex flex-col items-center">
                    <ToggleSwitch 
                        label="Control"
                        enabled={controlOn}
                        disabled={!calcOn}
                        onToggle={() => onToggle(model.prefix, mode, true, false)}
                        colorOn="bg-emerald-500"
                    />
                     <span className={`text-[10px] mt-2 font-bold uppercase tracking-wider ${controlOn ? 'text-emerald-400' : 'text-slate-600'}`}>
                        {controlOn ? "Active" : "Disabled"}
                    </span>
                </div>
            </div>
        </div>
    );
};
