import React from 'react';
import { useTagStore } from '../store/tagStore';

interface NextRunDisplayProps {
  systemPrefix: string;
}

export const NextRunDisplay: React.FC<NextRunDisplayProps> = ({ systemPrefix }) => {
  const nextRunValue = useTagStore(state => {
    const key = `${systemPrefix}:NextRun`;
    // We only care about the value property to minimize re-renders
    return state.tags[key]?.value;
  });

  // If the value isn't available yet or is invalid, show a placeholder or nothing
  // Showing nothing is safer to avoid UI clutter
  if (typeof nextRunValue !== 'number') return null;

  return (
    <div className="flex flex-col items-center min-w-[60px]">
      <span className="text-[10px] uppercase font-bold text-slate-400 tracking-wider mb-1">Next Run</span>
      <div className="flex items-baseline gap-1">
        <span className="text-2xl font-mono font-bold text-indigo-400 leading-none">
          {Math.round(nextRunValue)}
        </span>
        <span className="text-xs text-slate-500 font-bold">s</span>
      </div>
    </div>
  );
};
