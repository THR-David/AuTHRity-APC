import React, { useState } from 'react';
import { ValueEditorModal } from './ValueEditorModal';
import { useTagStore } from '../store/tagStore';
import { useWriteNode } from '../hooks/useWriteNode';

interface TuningTabProps {
  cvList: string[];
  mvList: string[];
  descriptions: Record<string, string>;
  wsRef: React.RefObject<WebSocket | null>;
  systemPrefix: string;
}

interface EditingField {
  nodeId: string;
  title: string;
  currentValue: number;
  unit?: string;
}

// Optimized row component with selective Zustand subscriptions
const CVTuningRow: React.FC<{
  cv: string;
  description?: string;
  wsRef: React.RefObject<WebSocket | null>;
}> = ({ cv, description, wsRef }) => {
  const weight = useTagStore(s => s.tags[`${cv}:Weight`]?.value);
  const alpha = useTagStore(s => s.tags[`${cv}:Alpha`]?.value);
  const lowLimit = useTagStore(s => s.tags[`${cv}:LowLimit`]?.value);
  const highLimit = useTagStore(s => s.tags[`${cv}:HighLimit`]?.value);
  const lowLowLimit = useTagStore(s => s.tags[`${cv}:LowLowLimit`]?.value);
  const highHighLimit = useTagStore(s => s.tags[`${cv}:HighHighLimit`]?.value);

  const [editingField, setEditingField] = useState<EditingField | null>(null);

  const { isWriting, write } = useWriteNode(
    editingField?.nodeId || '',
    wsRef,
    {
      onSuccess: () => setEditingField(null)
    }
  );

  const handleFieldClick = (nodeId: string, title: string, currentValue: number) => {
    setEditingField({ nodeId, title, currentValue });
  };

  const formatValue = (value: any): string => {
    if (value === undefined || value === null) return "--";
    if (typeof value === "number") return value.toFixed(2);
    return String(value);
  };

  return (
    <>
      <tr className="hover:bg-slate-800/40 transition-colors group">
        <td className="px-3 py-1 font-mono text-[11px] text-slate-400 group-hover:text-indigo-300 transition-colors truncate">{cv}</td>
        <td className="px-2 py-1 text-[11px] text-slate-400 truncate">{description || "--"}</td>
        
        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${cv}:Weight`, `${cv} - Weight`, Number(weight))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(weight)}
          </span>
        </td>

        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${cv}:Alpha`, `${cv} - Alpha Factor`, Number(alpha))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(alpha)}
          </span>
        </td>

        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${cv}:LowLimit`, `${cv} - Low Limit`, Number(lowLimit))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(lowLimit)}
          </span>
        </td>

        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${cv}:HighLimit`, `${cv} - High Limit`, Number(highLimit))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(highLimit)}
          </span>
        </td>

        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${cv}:LowLowLimit`, `${cv} - Low Low Limit`, Number(lowLowLimit))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(lowLowLimit)}
          </span>
        </td>

        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${cv}:HighHighLimit`, `${cv} - High High Limit`, Number(highHighLimit))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(highHighLimit)}
          </span>
        </td>
      </tr>

      {editingField && (
        <ValueEditorModal
          isOpen={true}
          onCancel={() => setEditingField(null)}
          title={editingField.title}
          currentValue={editingField.currentValue}
          onConfirm={write}
          isWriting={isWriting}
        />
      )}
    </>
  );
};

// Similar component for MV rows
const MVTuningRow: React.FC<{
  mv: string;
  description?: string;
  wsRef: React.RefObject<WebSocket | null>;
}> = ({ mv, description, wsRef }) => {
  const deltaMin = useTagStore(s => s.tags[`${mv}:DeltaMin`]?.value);
  const deltaMax = useTagStore(s => s.tags[`${mv}:DeltaMax`]?.value);
  const lowLimit = useTagStore(s => s.tags[`${mv}:LowLimit`]?.value);
  const highLimit = useTagStore(s => s.tags[`${mv}:HighLimit`]?.value);
  const weight = useTagStore(s => s.tags[`${mv}:Weight`]?.value);

  const [editingField, setEditingField] = useState<EditingField | null>(null);

  const { isWriting, write } = useWriteNode(
    editingField?.nodeId || '',
    wsRef,
    {
      onSuccess: () => setEditingField(null)
    }
  );

  const handleFieldClick = (nodeId: string, title: string, currentValue: number) => {
    setEditingField({ nodeId, title, currentValue });
  };

  const formatValue = (value: any): string => {
    if (value === undefined || value === null) return "--";
    if (typeof value === "number") return value.toFixed(2);
    return String(value);
  };

  return (
    <>
      <tr className="hover:bg-slate-800/40 transition-colors group">
        <td className="px-3 py-1 font-mono text-[11px] text-slate-400 group-hover:text-indigo-300 transition-colors truncate">{mv}</td>
        <td className="px-2 py-1 text-[11px] text-slate-400 truncate">{description || "--"}</td>
        
        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${mv}:DeltaMin`, `${mv} - Delta Min`, Number(deltaMin))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(deltaMin)}
          </span>
        </td>

        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${mv}:DeltaMax`, `${mv} - Delta Max`, Number(deltaMax))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(deltaMax)}
          </span>
        </td>

        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${mv}:LowLimit`, `${mv} - Low Limit`, Number(lowLimit))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(lowLimit)}
          </span>
        </td>

        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${mv}:HighLimit`, `${mv} - High Limit`, Number(highLimit))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(highLimit)}
          </span>
        </td>

        <td className="px-2 py-1 text-right text-xs">
          <span
            onClick={() => handleFieldClick(`${mv}:Weight`, `${mv} - Move Suppression Weight`, Number(weight))}
            className="inline-block cursor-pointer transition-all duration-150 hover:text-emerald-400 hover:scale-105 font-mono text-slate-300"
          >
            {formatValue(weight)}
          </span>
        </td>
      </tr>

      {editingField && (
        <ValueEditorModal
          isOpen={true}
          onCancel={() => setEditingField(null)}
          title={editingField.title}
          currentValue={editingField.currentValue}
          onConfirm={write}
          isWriting={isWriting}
        />
      )}
    </>
  );
};

export const TuningTab: React.FC<TuningTabProps> = ({ 
  cvList, 
  mvList, 
  descriptions,
  wsRef,
  systemPrefix
}) => {
  const predictionHorizon = useTagStore(s => s.tags[`${systemPrefix}:PredictionHorizon`]?.value);
  const controlHorizon = useTagStore(s => s.tags[`${systemPrefix}:ControlHorizon`]?.value);
  
  const [editingControlHorizon, setEditingControlHorizon] = useState<EditingField | null>(null);
  
  const { isWriting: isWritingM, write: writeM } = useWriteNode(
    `${systemPrefix}:ControlHorizon`,
    wsRef,
    {
      onSuccess: () => setEditingControlHorizon(null)
    }
  );
  
  const handleControlHorizonClick = () => {
    if (typeof controlHorizon === 'number') {
      setEditingControlHorizon({
        nodeId: `${systemPrefix}:ControlHorizon`,
        title: 'Control Horizon (M)',
        currentValue: controlHorizon
      });
    }
  };
  
  const formatValue = (value: any): string => {
    if (value === undefined || value === null) return "--";
    if (typeof value === "number") return value.toFixed(0);
    return String(value);
  };

  return (
    <div className="space-y-6 animate-in fade-in duration-300">
      
      {/* CV TUNING TABLE */}
      <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
        <div className="bg-slate-800/80 px-4 py-2 border-b-2 border-slate-700">
          <h3 className="font-extrabold text-slate-100 text-[13px] tracking-wide uppercase">
            Controlled Variables (CVs) - Tuning Parameters
          </h3>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs table-fixed">
            <thead>
              <tr className="border-b-2 border-slate-700 bg-slate-900/60 text-slate-300">
                <th className="px-3 py-2 font-extrabold uppercase tracking-wider text-[13px] w-24">Tag</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-left text-[13px] w-40">Description</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">Weight</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">Alpha</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">Low Limit</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">High Limit</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">Low Low</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">High High</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/50">
              {cvList.map((cv) => (
                <CVTuningRow
                  key={cv}
                  cv={cv}
                  description={descriptions[cv]}
                  wsRef={wsRef}
                />
              ))}
            </tbody>
          </table>
        </div>
      </div>

      {/* MV TUNING TABLE */}
      <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
        <div className="bg-slate-800/80 px-4 py-2 border-b-2 border-slate-700">
          <h3 className="font-extrabold text-slate-100 text-[13px] tracking-wide uppercase">
            Manipulated Variables (MVs) - Tuning Parameters
          </h3>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs table-fixed">
            <thead>
              <tr className="border-b-2 border-slate-700 bg-slate-900/60 text-slate-300">
                <th className="px-3 py-2 font-extrabold uppercase tracking-wider text-[13px] w-24">Tag</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-left text-[13px] w-40">Description</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">Delta Min</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">Delta Max</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">Low Limit</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">High Limit</th>
                <th className="px-2 py-2 font-extrabold uppercase tracking-wider text-right text-[13px]">Weight</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/50">
              {mvList.map((mv) => (
                <MVTuningRow
                  key={mv}
                  mv={mv}
                  description={descriptions[mv]}
                  wsRef={wsRef}
                />
              ))}
            </tbody>
          </table>
        </div>
      </div>

      {/* GLOBAL CONTROLLER PARAMETERS */}
      <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
        <div className="bg-slate-800/80 px-4 py-2 border-b-2 border-slate-700">
          <h3 className="font-extrabold text-slate-100 text-[13px] tracking-wide uppercase">
            ⚙️ Global Controller Parameters
          </h3>
        </div>

        <div className="p-4">
          <div className="grid grid-cols-2 gap-6">
            {/* Prediction Horizon (read-only reference) */}
            <div>
              <label className="block text-[11px] font-bold text-slate-400 uppercase mb-2">
                Prediction Horizon (P)
              </label>
              <div className="w-full bg-slate-800/50 border border-slate-700 rounded px-3 py-2 text-sm text-slate-400 font-mono text-center">
                {formatValue(predictionHorizon)}
              </div>
              <p className="text-[10px] text-slate-500 mt-1 text-center">Steps (reference only)</p>
            </div>

            {/* Control Horizon (editable) */}
            <div>
              <label className="block text-[11px] font-bold text-slate-400 uppercase mb-2">
                Control Horizon (M)
              </label>
              <div
                onClick={handleControlHorizonClick}
                className="w-full bg-slate-800 border border-slate-600 rounded px-3 py-2 text-sm text-slate-200 font-bold text-center cursor-pointer hover:border-slate-500 hover:scale-105 transition-all"
                title="Number of future moves calculated (must be ≤ P)"
              >
                {formatValue(controlHorizon)}
              </div>
              <p className="text-[10px] text-slate-500 mt-1 text-center">Steps (click to edit)</p>
            </div>
          </div>

          <div className="mt-4 p-3 bg-slate-800/50 rounded border border-slate-700/50">
            <p className="text-[11px] text-slate-400 leading-relaxed">
              <span className="font-bold text-slate-200">📝 Tuning Guide:</span> Control Horizon (M) defines how many future control moves are optimized. 
              Lower values (M ≤ 5) provide aggressive control, higher values smoother control. Must satisfy: 1 ≤ M ≤ P.
            </p>
          </div>
        </div>
      </div>
      
      {editingControlHorizon && (
        <ValueEditorModal
          isOpen={true}
          onCancel={() => setEditingControlHorizon(null)}
          title={editingControlHorizon.title}
          currentValue={editingControlHorizon.currentValue}
          onConfirm={writeM}
          isWriting={isWritingM}
        />
      )}
    </div>
  );
};
