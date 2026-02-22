import React, { useState } from 'react';

interface MatrixEditorProps {
  title: string;
  rowLabels: string[]; // CV names
  colLabels: string[]; // MV names
  matrix: number[][];
  onChange: (row: number, col: number, value: number) => void;
  matrixType?: 'gain' | 'tau' | 'deadTime';
}

export const MatrixEditor: React.FC<MatrixEditorProps> = ({
  title,
  rowLabels,
  colLabels,
  matrix,
  onChange,
  matrixType = 'gain'
}) => {
  const [focusedCell, setFocusedCell] = useState<{ row: number; col: number } | null>(null);

  const getCellStyle = (value: number) => {
    if (matrixType === 'gain') {
      if (value === 0) return 'text-slate-600 opacity-40'; // Dim for zero
      if (value < 0) return 'text-red-400 font-semibold'; // Red for negative
      return 'text-emerald-400 font-semibold'; // Green for positive
    }
    return 'text-slate-300';
  };

  const isRowHighlighted = (row: number) => focusedCell?.row === row;
  const isColHighlighted = (col: number) => focusedCell?.col === col;

  return (
    <div className="bg-slate-900/50 rounded-xl border border-slate-800 overflow-hidden">
      {/* Header */}
      <div className="bg-slate-800/80 px-4 py-2 border-b border-slate-700">
        <h3 className="font-bold text-slate-200 text-sm uppercase tracking-wide">{title}</h3>
      </div>

      {/* Matrix Container with Scroll */}
      <div className="relative overflow-auto max-h-[600px]" style={{ scrollbarGutter: 'stable' }}>
        <table className="w-full text-xs border-collapse">
          <thead className="bg-slate-800/90 sticky top-0 z-20">
            <tr>
              {/* Top-left corner cell */}
              <th className="p-2 border border-slate-700 text-slate-500 font-medium sticky left-0 bg-slate-800/95 z-30">
                CV \ MV
              </th>
              {/* MV Headers (sticky at top) */}
              {colLabels.map((label, colIdx) => (
                <th
                  key={colIdx}
                  className={`p-2 border border-slate-700 text-center font-semibold text-slate-300 min-w-[80px] transition-colors ${
                    isColHighlighted(colIdx) ? 'bg-indigo-500/20' : ''
                  }`}
                >
                  {label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rowLabels.map((rowLabel, rowIdx) => (
              <tr key={rowIdx} className={`transition-colors ${isRowHighlighted(rowIdx) ? 'bg-indigo-500/10' : ''}`}>
                {/* CV Label (sticky on left) */}
                <th className="p-2 border border-slate-700 text-left font-semibold text-slate-300 bg-slate-800/80 sticky left-0 z-10 whitespace-nowrap">
                  {rowLabel}
                </th>
                {/* Matrix Cells */}
                {colLabels.map((_, colIdx) => {
                  const value = matrix[rowIdx]?.[colIdx] ?? 0;
                  const isFocused = focusedCell?.row === rowIdx && focusedCell?.col === colIdx;
                  
                  return (
                    <td
                      key={colIdx}
                      className={`p-1 border border-slate-700 text-center transition-all ${
                        isFocused ? 'bg-indigo-600/30 ring-2 ring-indigo-500' : 
                        (isRowHighlighted(rowIdx) || isColHighlighted(colIdx)) ? 'bg-indigo-500/10' : ''
                      }`}
                    >
                      <input
                        type="number"
                        step="any"
                        value={value}
                        onChange={(e) => {
                          const newValue = parseFloat(e.target.value) || 0;
                          onChange(rowIdx, colIdx, newValue);
                        }}
                        onFocus={() => setFocusedCell({ row: rowIdx, col: colIdx })}
                        onBlur={() => setFocusedCell(null)}
                        className={`w-full bg-transparent border-none text-center outline-none px-2 py-1 rounded ${getCellStyle(value)} focus:bg-slate-700/50 transition-colors [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none`}
                      />
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Legend */}
      {matrixType === 'gain' && (
        <div className="bg-slate-800/50 px-4 py-2 border-t border-slate-700 flex items-center gap-4 text-xs">
          <span className="text-slate-500">Legend:</span>
          <div className="flex items-center gap-1">
            <div className="w-3 h-3 bg-emerald-400 rounded"></div>
            <span className="text-slate-400">Positive</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-3 h-3 bg-red-400 rounded"></div>
            <span className="text-slate-400">Negative</span>
          </div>
          <div className="flex items-center gap-1">
            <div className="w-3 h-3 bg-slate-600 rounded opacity-40"></div>
            <span className="text-slate-400">Zero (No Effect)</span>
          </div>
        </div>
      )}
    </div>
  );
};
