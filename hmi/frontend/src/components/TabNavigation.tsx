import React from 'react';

interface TabProps {
  tabs: string[];
  activeTab: string;
  onTabChange: (tab: string) => void;
}

export const TabNavigation: React.FC<TabProps> = ({ tabs, activeTab, onTabChange }) => {
  return (
    // Removed the outer bg-slate-900 and border-b classes here.
    // We now just return the flex container of buttons.
    <div className="flex">
      {tabs.map((tab) => {
        const isActive = activeTab === tab;
        return (
          <button
            key={tab}
            onClick={() => onTabChange(tab)}
            className={`
              relative px-6 py-4 text-sm font-medium transition-colors
              ${isActive ? 'text-emerald-400' : 'text-slate-400 hover:text-slate-200'}
            `}
          >
            {tab}
            {/* Active Underline Indicator */}
            {isActive && (
              <div className="absolute bottom-0 left-0 w-full h-0.5 bg-emerald-500 shadow-[0_-2px_6px_rgba(16,185,129,0.5)]" />
            )}
          </button>
        );
      })}
    </div>
  );
};