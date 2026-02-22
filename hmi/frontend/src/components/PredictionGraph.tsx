import React, { useState, useMemo, useEffect, useRef } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import { useTagStore } from '../store/tagStore';

interface PredictionGraphProps {
  cvList: string[];
  mvList: string[];
  sampleTime?: number;
}

// Cause-and-Effect Layout (Stacked Graphs)
// Top Chart: CV Predictions (The goals)
// Bottom Chart: MV Future Plans (The actions)

export const PredictionGraph: React.FC<PredictionGraphProps> = React.memo(({ 
  cvList, 
  mvList, 
  sampleTime = 20 
}) => {
  // THROTTLED subscription - only re-render every 500ms max
  const [throttledData, setThrottledData] = useState<Record<string, any>>({});
  const lastUpdate = useRef(0);
  
  useTagStore(state => {
    const now = Date.now();
    if (now - lastUpdate.current > 500) { // Update max 2x per second
      lastUpdate.current = now;
      setThrottledData(state.tags);
    }
  });
  
  const data = throttledData;
  
  // Only select CVs that have non-zero weight (initialize once, then manual control)
  const [selectedCVs, setSelectedCVs] = useState<Set<string>>(new Set());
  const [selectedMVs, setSelectedMVs] = useState<Set<string>>(new Set());
  
  // Initialize selected CVs based on weight and MVs (only once when cvList/mvList change)
  useEffect(() => {
    if (cvList.length === 0 || mvList.length === 0) return;
    
    const initialCVs = cvList.filter(cv => {
      const weight = data[`${cv}:Weight`]?.value;
      return typeof weight === 'number' && weight !== 0;
    });
    
    // Only update if we found weighted CVs
    if (initialCVs.length > 0) {
      setSelectedCVs(new Set(initialCVs));
      setSelectedMVs(new Set(mvList));
    }
  }, [cvList, mvList]); // Only run when CV/MV lists change (model switch)

  const toggleCV = (cv: string) => {
    setSelectedCVs(prev => {
      const newSet = new Set(prev);
      if (newSet.has(cv)) newSet.delete(cv);
      else newSet.add(cv);
      return newSet;
    });
  };

  const toggleMV = (mv: string) => {
    setSelectedMVs(prev => {
      const newSet = new Set(prev);
      if (newSet.has(mv)) newSet.delete(mv);
      else newSet.add(mv);
      return newSet;
    });
  };

  const chartData = useMemo(() => {
    // Find the longest horizon
    let maxLength = 0;
    
    selectedCVs.forEach(cv => {
      const prediction = data[`${cv}:Prediction`]?.value;
      if (Array.isArray(prediction)) {
        maxLength = Math.max(maxLength, prediction.length);
      }
    });
    
    selectedMVs.forEach(mv => {
      const futurePlan = data[`${mv}:FuturePlan`]?.value;
      if (Array.isArray(futurePlan)) {
        maxLength = Math.max(maxLength, futurePlan.length);
      }
    });

    if (maxLength === 0) return [];

    // Build chart data points
    const points = [];
    for (let i = 0; i < maxLength; i++) {
      const point: any = { 
        step: i,
        time: (i * sampleTime) / 60 // Keep exact time in minutes
      };
      
      // Add CV predictions (raw values)
      selectedCVs.forEach(cv => {
        const prediction = data[`${cv}:Prediction`]?.value;
        if (Array.isArray(prediction) && i < prediction.length) {
          point[`${cv}_pred`] = prediction[i];
        }
      });
      
      // Add MV future plans (raw values)
      selectedMVs.forEach(mv => {
        const futurePlan = data[`${mv}:FuturePlan`]?.value;
        if (Array.isArray(futurePlan) && i < futurePlan.length) {
          point[`${mv}_plan`] = futurePlan[i];
        }
      });
      
      points.push(point);
    }
    
    return points;
  }, [selectedCVs, selectedMVs, data, sampleTime]);

  // Calculate Y-axis domain for CVs using operational limits
  const yAxisDomainCV = useMemo(() => {
    if (selectedCVs.size === 0) return [0, 10];
    
    let globalMin = Infinity;
    let globalMax = -Infinity;
    
    // Find the range that encompasses all selected CV limits
    selectedCVs.forEach(cv => {
      const lowLimit = data[`${cv}:LowLimit`]?.value;
      const highLimit = data[`${cv}:HighLimit`]?.value;
      
      if (typeof lowLimit === 'number' && typeof highLimit === 'number') {
        globalMin = Math.min(globalMin, lowLimit);
        globalMax = Math.max(globalMax, highLimit);
      }
    });
    
    // Fallback if no limits found
    if (globalMin === Infinity || globalMax === -Infinity) {
      return [0, 10];
    }
    
    // Add small padding (2%) for visual clarity
    const range = globalMax - globalMin;
    const padding = range * 0.02;
    return [globalMin - padding, globalMax + padding];
  }, [data, selectedCVs]);

  // Calculate Y-axis domain for MVs using operational limits
  const yAxisDomainMV = useMemo(() => {
    if (selectedMVs.size === 0) return [0, 10];
    
    let globalMin = Infinity;
    let globalMax = -Infinity;
    
    // Find the range that encompasses all selected MV limits
    selectedMVs.forEach(mv => {
      const lowLimit = data[`${mv}:LowLimit`]?.value;
      const highLimit = data[`${mv}:HighLimit`]?.value;
      
      if (typeof lowLimit === 'number' && typeof highLimit === 'number') {
        globalMin = Math.min(globalMin, lowLimit);
        globalMax = Math.max(globalMax, highLimit);
      }
    });
    
    // Fallback if no limits found
    if (globalMin === Infinity || globalMax === -Infinity) {
      return [0, 10];
    }
    
    // Add small padding (2%) for visual clarity
    const range = globalMax - globalMin;
    const padding = range * 0.02;
    return [globalMin - padding, globalMax + padding];
  }, [data, selectedMVs]);

  const cvColors = ['#6366f1', '#8b5cf6', '#ec4899', '#f43f5e'];
  const mvColors = ['#f59e0b', '#eab308', '#84cc16', '#10b981'];

  return (
    <div className="bg-slate-900/50 rounded-xl border border-slate-800 overflow-hidden shadow-sm">
      <div className="bg-slate-900/80 px-3 py-1.5 border-b border-slate-800">
        <h3 className="font-bold text-slate-200 text-xs tracking-wide">Prediction & Control Horizon</h3>
      </div>

      <div className="p-4 flex flex-col gap-4">
        {/* Variable Selection Row */}
        <div className="flex gap-6">
          {/* CVs */}
          <div className="flex-1">
            <p className="text-[10px] uppercase font-bold text-indigo-400 mb-2">Controlled Variables</p>
            <div className="flex flex-wrap gap-2">
              {cvList.map((cv, idx) => (
                <button
                  key={cv}
                  onClick={() => toggleCV(cv)}
                  className={`px-2 py-1 text-xs rounded-md transition-colors ${
                    selectedCVs.has(cv)
                      ? 'bg-indigo-600 text-white'
                      : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
                  }`}
                  style={selectedCVs.has(cv) ? { backgroundColor: cvColors[idx % cvColors.length] } : {}}
                >
                  {cv}
                </button>
              ))}
            </div>
          </div>

          {/* MVs */}
          <div className="flex-1">
            <p className="text-[10px] uppercase font-bold text-amber-400 mb-2">Manipulated Variables</p>
            <div className="flex flex-wrap gap-2">
              {mvList.map((mv, idx) => (
                <button
                  key={mv}
                  onClick={() => toggleMV(mv)}
                  className={`px-2 py-1 text-xs rounded-md transition-colors ${
                    selectedMVs.has(mv)
                      ? 'bg-amber-600 text-white'
                      : 'bg-slate-800 text-slate-400 hover:bg-slate-700'
                  }`}
                  style={selectedMVs.has(mv) ? { backgroundColor: mvColors[idx % mvColors.length] } : {}}
                >
                  {mv}
                </button>
              ))}
            </div>
          </div>
        </div>

        {/* Charts Container */}
        {chartData.length > 0 ? (
          <div className="flex flex-col h-[500px]">
             {/* CV Chart (Top) - 60% Height */}
             <div className="h-[60%] w-full relative">
                <div className="absolute top-2 right-2 z-10 text-[10px] font-bold text-indigo-400/50 uppercase pointer-events-none">Outcomes</div>
                <ResponsiveContainer width="100%" height="100%">
                    <LineChart data={chartData} syncId="processSync">
                    <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
                    {/* Hide X axis labels on top chart to avoid clutter */}
                    <XAxis 
                        dataKey="time" 
                        hide={true} 
                        type="number" 
                        domain={['dataMin', 'dataMax']} 
                    />
                    <YAxis 
                        stroke="#94a3b8"
                        tick={{ fontSize: 11 }}
                        label={{ value: 'Value', angle: -90, position: 'insideLeft', fill: '#94a3b8', fontSize: 10 }}
                        domain={yAxisDomainCV}
                        tickFormatter={(value) => value.toFixed(1)}
                    />
                    <Tooltip 
                        contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px', fontSize: '11px' }}
                        labelStyle={{ color: '#cbd5e1' }}
                        formatter={(value: any, name: any) => {
                            if (typeof value !== 'number') return [value, name];
                            return [value.toFixed(2), name];
                        }}
                    />
                    <Legend wrapperStyle={{ fontSize: '11px' }} />
                    
                    {Array.from(selectedCVs).map((cv, idx) => (
                        <Line
                        key={`${cv}_pred`}
                        type="monotone"
                        dataKey={`${cv}_pred`}
                        stroke={cvColors[idx % cvColors.length]}
                        strokeWidth={2}
                        dot={false}
                        name={`${cv}`}
                        animationDuration={300}
                        />
                    ))}
                    </LineChart>
                </ResponsiveContainer>
             </div>

             {/* MV Chart (Bottom) - 40% Height */}
             <div className="h-[40%] w-full relative -mt-2">
                <div className="absolute top-2 right-2 z-10 text-[10px] font-bold text-amber-400/50 uppercase pointer-events-none">Actions</div>
                <ResponsiveContainer width="100%" height="100%">
                    <LineChart data={chartData} syncId="processSync">
                    <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
                    <XAxis 
                        dataKey="time" 
                        stroke="#94a3b8"
                        label={{ value: 'Minutes', position: 'insideBottom', offset: -5, fill: '#94a3b8', fontSize: 10 }}
                        tick={{ fontSize: 11 }}
                        type="number"
                        domain={['dataMin', 'dataMax']}
                        tickFormatter={(value) => Math.round(value).toString()}
                        allowDecimals={false}
                    />
                    <YAxis 
                        stroke="#94a3b8"
                        tick={{ fontSize: 11 }}
                        domain={yAxisDomainMV}
                        tickFormatter={(value) => value.toFixed(1)}
                    />
                    <Tooltip 
                        contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '8px', fontSize: '11px' }}
                        labelStyle={{ color: '#cbd5e1' }}
                        formatter={(value: any, name: any) => {
                            if (typeof value !== 'number') return [value, name];
                            return [value.toFixed(2), name];
                        }}
                    />
                    <Legend wrapperStyle={{ fontSize: '11px' }} />
                    
                    {Array.from(selectedMVs).map((mv, idx) => (
                        <Line
                        key={`${mv}_plan`}
                        type="stepAfter"
                        dataKey={`${mv}_plan`}
                        stroke={mvColors[idx % mvColors.length]}
                        strokeWidth={2}
                        dot={false}
                        name={`${mv}`}
                        animationDuration={300}
                        />
                    ))}
                    </LineChart>
                </ResponsiveContainer>
             </div>
          </div>
        ) : (
          <div className="h-64 flex items-center justify-center text-slate-500 text-sm">
            No prediction data available. Enable Calculator mode to see predictions.
          </div>
        )}
      </div>
    </div>
  );
});
