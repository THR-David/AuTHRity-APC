import React, { useState, useEffect, useRef } from 'react';
import { Loader2 } from 'lucide-react';

interface ValueEditorModalProps {
  isOpen: boolean;
  title: string;
  currentValue: number | string;
  unit?: string;
  isWriting?: boolean;
  deniedReason?: string | null;
  onConfirm: (newValue: number) => void;
  onCancel: () => void;
}

export const ValueEditorModal: React.FC<ValueEditorModalProps> = ({
  isOpen,
  title,
  currentValue,
  unit = "",
  isWriting = false,
  deniedReason = null,
  onConfirm,
  onCancel
}) => {
  const [inputValue, setInputValue] = useState<string>("");
  const initialValueRef = useRef<number | string>(currentValue);

  // CRUCIAL FIX: Only sync input on initial open, NOT on background updates
  useEffect(() => {
    if (isOpen) {
      const value = typeof currentValue === 'number' ? currentValue.toFixed(2) : String(currentValue);
      setInputValue(value);
      initialValueRef.current = currentValue;
    }
  }, [isOpen]); // Removed currentValue from deps - only runs on open/close

  const handleConfirm = () => {
    if (deniedReason) {
      return;
    }
    const numValue = parseFloat(inputValue);
    if (!isNaN(numValue) && !isWriting) {
      onConfirm(numValue);
      // Don't close here! Parent will close when write succeeds
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !isWriting) {
      handleConfirm();
    } else if (e.key === 'Escape' && !isWriting) {
      onCancel();
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-in fade-in duration-200">
      <div className="bg-slate-900 border-2 border-slate-700 rounded-xl shadow-2xl w-96 animate-in zoom-in-95 duration-200">
        {/* Header */}
        <div className="bg-gradient-to-r from-indigo-900 to-indigo-800 px-4 py-3 border-b-2 border-slate-700 rounded-t-xl">
          <h3 className="font-bold text-slate-100 text-sm uppercase tracking-wide flex items-center gap-2">
            Edit Value
            {isWriting && <Loader2 className="w-4 h-4 animate-spin text-indigo-300" />}
          </h3>
        </div>

        {/* Body */}
        <div className="p-6">
          <label className="block text-xs font-bold text-slate-400 uppercase mb-2">
            {title}
          </label>
          
          <div className="flex items-center gap-2 mb-4">
            <input
              type="text"
              value={inputValue}
              onChange={(e) => setInputValue(e.target.value)}
              onKeyDown={handleKeyDown}
              disabled={isWriting || !!deniedReason}
              autoFocus
              className={`flex-1 bg-slate-800 border-2 rounded px-4 py-3 text-lg font-bold text-center focus:outline-none transition-all ${
                deniedReason
                  ? 'border-rose-500/50 text-rose-300 cursor-not-allowed'
                  : isWriting 
                  ? 'border-amber-500/50 text-amber-400 cursor-wait' 
                  : 'border-indigo-600/50 text-indigo-300 focus:ring-2 focus:ring-indigo-500 focus:border-indigo-500'
              }`}
              placeholder="Enter value"
            />
            {unit && (
              <span className="text-slate-400 font-semibold text-sm">{unit}</span>
            )}
          </div>

          <div className="bg-slate-800/50 rounded p-3 mb-4">
            <div className="flex justify-between text-xs">
              <span className="text-slate-500">Current:</span>
              <span className="text-cyan-400 font-semibold">
                {typeof initialValueRef.current === 'number' ? initialValueRef.current.toFixed(2) : initialValueRef.current} {unit}
              </span>
            </div>
          </div>

          {deniedReason && (
            <div className="bg-rose-900/30 border border-rose-700/50 rounded p-3 mb-4 text-xs text-rose-300">
              Access denied: {deniedReason}
            </div>
          )}

          {/* Buttons */}
          <div className="flex gap-3">
            <button
              onClick={onCancel}
              disabled={isWriting}
              className="flex-1 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-slate-200 font-semibold text-sm rounded transition-all disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Cancel
            </button>
            <button
              onClick={handleConfirm}
              disabled={isWriting || !!deniedReason}
              className="flex-1 px-4 py-2 bg-indigo-600 hover:bg-indigo-500 text-white font-bold text-sm rounded transition-all shadow-lg hover:shadow-indigo-500/50 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2"
            >
              {isWriting ? (
                <>
                  <Loader2 className="w-4 h-4 animate-spin" />
                  Writing...
                </>
              ) : deniedReason ? (
                'Not Allowed'
              ) : (
                '✓ Confirm'
              )}
            </button>
          </div>

          <p className="text-[10px] text-slate-500 mt-3 text-center">
            {deniedReason
              ? 'Your role does not have permission for this write'
              : isWriting
                ? 'Writing to server...'
                : 'Press Enter to confirm • Esc to cancel'}
          </p>
        </div>
      </div>
    </div>
  );
};
