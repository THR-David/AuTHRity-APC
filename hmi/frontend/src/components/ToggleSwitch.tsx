import React from 'react';

interface ToggleProps {
  label: string;
  enabled: boolean;
  onToggle: (newState: boolean) => void;
  colorOn?: string;
  disabled?: boolean;
}

export const ToggleSwitch: React.FC<ToggleProps> = ({ label, enabled, onToggle, colorOn = "bg-emerald-500", disabled = false }) => {
  return (
    <div className="flex flex-col items-center gap-1">
      <span className={`text-[10px] uppercase font-bold tracking-wider ${disabled ? 'text-slate-600' : 'text-slate-400'}`}>{label}</span>
      <button 
        onClick={() => !disabled && onToggle(!enabled)}
        disabled={disabled}
        className={`w-12 h-6 rounded-full p-1 transition-colors duration-300 ${
          disabled ? 'bg-slate-800 cursor-not-allowed opacity-50' : 
          enabled ? colorOn : 'bg-slate-700'
        }`}
      >
        <div className={`bg-white w-4 h-4 rounded-full shadow-md transform transition-transform duration-300 ${enabled ? 'translate-x-6' : 'translate-x-0'}`} />
      </button>
    </div>
  );
};