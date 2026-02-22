import React, { useState, useEffect } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';

interface ModelMetadata {
  name: string;
  description?: string;
  version?: string;
  model_type?: string;
}

interface ModelTuning {
  prediction_horizon: number;
  control_horizon: number;
  sample_time: number;
  solver_tolerance?: number;
  max_iterations?: number;
}

interface CVModel {
  name: string;
  description?: string;
  units?: string;
  weight?: number;
  alpha?: number;
  limits?: {
    low_low?: number;
    low?: number;
    target?: number;
    high?: number;
    high_high?: number;
  };
}

interface MVModel {
  name: string;
  description?: string;
  units?: string;
  weight_r?: number;
  max_move?: number;
  limits?: {
    low_low?: number;
    low?: number;
    high?: number;
    high_high?: number;
  };
}

interface DVModel {
  name: string;
  description?: string;
  units?: string;
}

interface PhysicsData {
  gain?: number[][];
  tau?: number[][];
  dead_time?: number[][];
  gain_dv?: number[][];
  tau_dv?: number[][];
  dead_time_dv?: number[][];
  step_coefficients?: number[][][]; // CV x MV x time_steps
  dv_coefficients?: number[][][];   // CV x DV x time_steps
}

interface PhysicsModel {
  metadata: ModelMetadata;
  tuning: ModelTuning;
  variables: {
    cvs: CVModel[];
    mvs: MVModel[];
    dvs?: DVModel[];
  };
  physics?: PhysicsData;
}

interface SelectedCell {
  cvIdx: number;
  inputIdx: number;
  inputType: 'MV' | 'DV';
  cvName: string;
  inputName: string;
  chartData: Array<{ time: number; gain: number }>;
}

export const ModelsTab: React.FC = () => {
  const [model, setModel] = useState<PhysicsModel | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedCell, setSelectedCell] = useState<SelectedCell | null>(null);
  const [compactMode, setCompactMode] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [hoveredCell, setHoveredCell] = useState<{ cvIdx: number; inputIdx: number; inputType: 'MV' | 'DV' } | null>(null);

  const fetchModel = async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await fetch('/api/controller/model');
      if (!response.ok) {
        if (response.status === 404) {
          throw new Error('No active controller found');
        }
        throw new Error(`Failed to fetch model: ${response.status}`);
      }
      const data = await response.json();
      setModel(data);
    } catch (err: any) {
      setError(err.message || 'Failed to load model');
      console.error('Model fetch error:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchModel();
  }, []);

  if (loading) {
    return (
      <div className="h-full flex items-center justify-center">
        <div className="text-center">
          <div className="inline-block animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500 mb-4"></div>
          <p className="text-slate-400">Loading active controller model...</p>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="h-full flex items-center justify-center">
        <div className="text-center">
          <span className="text-4xl mb-4 block">⚠️</span>
          <h3 className="text-xl font-bold mb-2 text-amber-400">No Active Controller</h3>
          <p className="text-slate-400 mb-4">{error}</p>
          <button
            onClick={fetchModel}
            className="bg-blue-600 hover:bg-blue-500 text-white px-4 py-2 rounded text-sm transition-colors"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  if (!model) {
    return (
      <div className="h-full flex items-center justify-center text-slate-500">
        No model data available
      </div>
    );
  }

  const cvCount = model.variables.cvs.length;
  const mvCount = model.variables.mvs.length;
  const dvCount = model.variables.dvs?.length || 0;

  const cellHeight = compactMode ? 'h-24' : 'h-32';
  const cellWidth = compactMode ? 'w-32' : 'w-40';

  // Filter logic: show all relationships involving matching variables
  const mvMatches = model.variables.mvs.filter(mv => 
    mv.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    mv.description?.toLowerCase().includes(searchTerm.toLowerCase())
  );
  
  const dvMatches = model.variables.dvs?.filter(dv => 
    dv.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    dv.description?.toLowerCase().includes(searchTerm.toLowerCase())
  ) || [];
  
  const cvMatches = model.variables.cvs.filter(cv => 
    cv.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    cv.description?.toLowerCase().includes(searchTerm.toLowerCase())
  );

  // If search is active, show:
  // - All CVs if an input matches (to see all relationships with that input)
  // - All inputs if a CV matches (to see all relationships with that CV)
  // - Only matches if both sides have matches
  const hasInputMatch = mvMatches.length > 0 || dvMatches.length > 0;
  const hasCvMatch = cvMatches.length > 0;
  
  const filteredCVs = searchTerm === '' 
    ? model.variables.cvs 
    : (hasInputMatch && !hasCvMatch ? model.variables.cvs : cvMatches);
  
  const filteredMVs = searchTerm === ''
    ? model.variables.mvs
    : (hasCvMatch && !hasInputMatch ? model.variables.mvs : mvMatches);
  
  const filteredDVs = searchTerm === ''
    ? (model.variables.dvs || [])
    : (hasCvMatch && !hasInputMatch ? (model.variables.dvs || []) : dvMatches);

  // Helper: Generate step response curve from FOPDT parameters
  const generateStepResponse = (gain: number, tau: number, deadTime: number, sampleTime: number, horizon: number): number[] => {
    const response: number[] = [];
    for (let i = 0; i < horizon; i++) {
      const t = i * sampleTime;
      if (t < deadTime) {
        response.push(0);
      } else {
        const value = gain * (1 - Math.exp(-(t - deadTime) / tau));
        response.push(value);
      }
    }
    return response;
  };

  // Helper: Get step response coefficients (from step_coefficients or generated from FOPDT)
  const getStepResponse = (cvIdx: number, inputIdx: number, inputType: 'MV' | 'DV'): number[] => {
    if (!model?.physics) return [];
    
    // Check if we have step_coefficients (step_response model)
    if (inputType === 'MV' && model.physics.step_coefficients) {
      return model.physics.step_coefficients[cvIdx]?.[inputIdx] || [];
    }
    if (inputType === 'DV' && model.physics.dv_coefficients) {
      return model.physics.dv_coefficients[cvIdx]?.[inputIdx] || [];
    }
    
    // Otherwise generate from parametric model (FOPDT)
    const gain = inputType === 'MV' 
      ? model.physics.gain?.[cvIdx]?.[inputIdx] ?? 0
      : model.physics.gain_dv?.[cvIdx]?.[inputIdx] ?? 0;
    
    const tau = inputType === 'MV'
      ? model.physics.tau?.[cvIdx]?.[inputIdx] ?? 60
      : model.physics.tau_dv?.[cvIdx]?.[inputIdx] ?? 60;
    
    const deadTime = inputType === 'MV'
      ? model.physics.dead_time?.[cvIdx]?.[inputIdx] ?? 0
      : model.physics.dead_time_dv?.[cvIdx]?.[inputIdx] ?? 0;
    
    if (gain === 0) return [];
    
    return generateStepResponse(gain, tau, deadTime, model.tuning.sample_time, model.tuning.prediction_horizon);
  };

  // Helper: Check if interaction has data
  const hasData = (coeffs: number[]): boolean => {
    return coeffs.length > 0 && coeffs.some(v => Math.abs(v) > 1e-10);
  };

  const renderInteractionDetail = () => {
    if (!selectedCell) return null;

    const { cvName, inputName, inputType, chartData, cvIdx, inputIdx } = selectedCell;
    
    // Calculate metrics
    const finalGain = chartData.length > 0 ? chartData[chartData.length - 1].gain : 0;
    const initialGain = chartData.length > 1 ? chartData[1].gain : 0;
    
    // Find 95% settling time
    const steadyStateValue = finalGain;
    const threshold = Math.abs(steadyStateValue * 0.95);
    let settlingIndex = chartData.findIndex(pt => Math.abs(pt.gain) >= threshold);
    const settlingTime = settlingIndex >= 0 ? chartData[settlingIndex].time : chartData[chartData.length - 1]?.time || 0;

    return (
      <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50 backdrop-blur-sm animate-in fade-in duration-200"
           onClick={() => setSelectedCell(null)}>
        <div className="bg-slate-900 border-2 border-blue-500 rounded-xl p-6 max-w-4xl w-full max-h-[90vh] overflow-y-auto shadow-2xl"
             onClick={(e) => e.stopPropagation()}>
          <div className="flex items-start justify-between mb-4">
            <div>
              <h3 className="text-2xl font-bold text-white mb-1">
                {inputName} → {cvName}
              </h3>
              <p className="text-sm text-slate-400">
                {inputType} step response | {chartData.length} points @ {model.tuning.sample_time}s intervals
              </p>
            </div>
            <button
              onClick={() => setSelectedCell(null)}
              className="text-slate-400 hover:text-white transition-colors text-3xl leading-none"
            >
              ×
            </button>
          </div>

          {/* Large Step Response Chart */}
          <div className="bg-slate-950 rounded-lg border border-slate-700 p-4 mb-6" style={{ height: '400px' }}>
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={chartData} margin={{ top: 20, right: 30, bottom: 40, left: 60 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
                <XAxis 
                  dataKey="time" 
                  stroke="#94a3b8"
                  label={{ value: 'Time (s)', position: 'bottom', offset: 0, fill: '#94a3b8' }}
                />
                <YAxis 
                  stroke="#94a3b8"
                  label={{ value: `Gain (${model.variables.cvs[cvIdx]?.units || 'CV'}/${inputType === 'MV' ? model.variables.mvs[inputIdx]?.units : model.variables.dvs?.[inputIdx]?.units || 'input'})`, angle: -90, position: 'insideLeft', fill: '#94a3b8' }}
                />
                <Tooltip 
                  contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #475569', borderRadius: '8px' }}
                  labelStyle={{ color: '#94a3b8' }}
                />
                <Line 
                  type="monotone" 
                  dataKey="gain" 
                  stroke={inputType === 'MV' ? '#fbbf24' : '#ec4899'} 
                  strokeWidth={3} 
                  dot={{ fill: inputType === 'MV' ? '#fbbf24' : '#ec4899', r: 4 }}
                />
              </LineChart>
            </ResponsiveContainer>
          </div>

          {/* Step Response Metrics */}
          <div className="grid grid-cols-3 gap-4">
            <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
              <div className="text-xs uppercase font-bold text-slate-400 mb-2">Initial Gain</div>
              <div className={`text-3xl font-bold font-mono ${initialGain > 0 ? 'text-emerald-400' : initialGain < 0 ? 'text-rose-400' : 'text-slate-500'}`}>
                {initialGain.toFixed(4)}
              </div>
            </div>

            <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
              <div className="text-xs uppercase font-bold text-slate-400 mb-2">Final Gain</div>
              <div className={`text-3xl font-bold font-mono ${finalGain > 0 ? 'text-emerald-400' : finalGain < 0 ? 'text-rose-400' : 'text-slate-500'}`}>
                {finalGain.toFixed(4)}
              </div>
            </div>

            <div className="bg-slate-800 rounded-lg p-4 border border-slate-700">
              <div className="text-xs uppercase font-bold text-slate-400 mb-2">Steady State Time</div>
              <div className="text-3xl font-bold text-blue-400 font-mono">
                {Math.floor(settlingTime / 60)} min
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  };

  return (
    <div className="space-y-6 animate-in fade-in duration-300">
      
      <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
        <div className="p-4">
          <div className="flex items-start justify-between">
            <div className="flex-1">
              <div className="flex items-center gap-3 mb-2">
                <h2 className="text-2xl font-bold text-slate-100">{model.metadata.name}</h2>
                <span className="px-2 py-1 bg-emerald-900/30 border border-emerald-700/40 text-emerald-400 text-xs font-bold rounded uppercase tracking-wide">
                  ✓ Active
                </span>
                {/* Compact dimension badges */}
                <div className="flex items-center gap-2 ml-4 text-xs">
                  <span className="px-2 py-1 bg-slate-800 border border-slate-700 rounded text-slate-300 font-bold">
                    {cvCount} CVs
                  </span>
                  <span className="px-2 py-1 bg-slate-800 border border-slate-700 rounded text-slate-300 font-bold">
                    {mvCount} MVs
                  </span>
                  {dvCount > 0 && (
                    <span className="px-2 py-1 bg-slate-800 border border-slate-700 rounded text-slate-300 font-bold">
                      {dvCount} DVs
                    </span>
                  )}
                  <span className="px-2 py-1 bg-slate-800 border border-slate-700 rounded text-slate-300 font-bold">
                    {cvCount * (mvCount + dvCount)} interactions
                  </span>
                </div>
              </div>
              {model.metadata.description && (
                <p className="text-slate-400 text-sm mb-2">{model.metadata.description}</p>
              )}
              <div className="flex items-center gap-6 text-xs text-slate-500">
                {model.metadata.version && (
                  <span>Version: <span className="font-mono font-bold text-slate-400">{model.metadata.version}</span></span>
                )}
                {model.metadata.model_type && (
                  <span>Type: <span className="font-bold capitalize text-slate-400">{model.metadata.model_type}</span></span>
                )}
              </div>
            </div>

            <div className="flex gap-2">
              <button
                onClick={() => setCompactMode(!compactMode)}
                className="bg-slate-800 hover:bg-slate-700 border border-slate-700 text-slate-300 px-3 py-2 rounded text-xs font-bold transition-colors"
                title="Toggle compact view"
              >
                {compactMode ? '📐 Normal' : '📏 Compact'}
              </button>
              <button
                onClick={fetchModel}
                className="bg-slate-800 hover:bg-slate-700 border border-slate-700 text-slate-300 px-3 py-2 rounded text-xs font-bold transition-colors"
                title="Refresh model data"
              >
                🔄 Refresh
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Search and Legend */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex-1">
          <input
            type="text"
            placeholder="🔍 Search variables (CV, MV, DV names)..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="w-full bg-slate-900 border border-slate-700 rounded-lg px-4 py-2 text-sm text-slate-200 placeholder-slate-500 focus:border-blue-500 focus:outline-none transition-colors"
          />
        </div>
        <div className="flex items-center gap-4 text-xs">
          <span className="text-slate-400 font-bold">LEGEND:</span>
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 bg-amber-500 rounded"></div>
            <span className="text-slate-300">MV (Manipulated)</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 bg-pink-500 rounded"></div>
            <span className="text-slate-300">DV (Disturbance)</span>
          </div>
        </div>
      </div>

      {/* MODEL MATRIX - ALL INPUTS (MVs + DVs) → CVs */}
      <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700 overflow-hidden shadow-lg">
        <div className="bg-gradient-to-r from-slate-800 to-slate-700 px-4 py-3 border-b-2 border-slate-700">
          <h3 className="font-extrabold text-slate-100 text-[13px] tracking-wide uppercase">
            📊 Model Matrix - Step Response Curves
          </h3>
          <p className="text-xs text-slate-300 mt-1">
            Click any graph to view details. Rows = CVs, Columns = Inputs (MVs + DVs)
          </p>
        </div>

        <div className="overflow-x-auto p-4">
          <div className="inline-block min-w-full">
            {/* Header Row */}
            <div className="flex gap-1 mb-1">
              <div className={`${compactMode ? 'w-24' : 'w-32'} flex-shrink-0 px-2 py-2 font-bold uppercase text-[10px] text-slate-400`}>
                CV \ Input →
              </div>
              {/* MV Headers */}
              {filteredMVs.map((mv) => (
                <div 
                  key={mv.name} 
                  className={`${cellWidth} flex-shrink-0 px-2 py-2 font-bold text-[9px] text-amber-400 text-center bg-amber-900/10 rounded`}
                  style={{ writingMode: compactMode ? 'vertical-rl' : 'horizontal-tb', transform: compactMode ? 'none' : 'rotate(-45deg)', transformOrigin: 'center' }}
                  title={`${mv.name}${mv.description ? ' - ' + mv.description : ''}`}
                >
                  {mv.name}
                </div>
              ))}
              {/* DV Headers */}
              {filteredDVs.map((dv) => (
                <div 
                  key={dv.name} 
                  className={`${cellWidth} flex-shrink-0 px-2 py-2 font-bold text-[9px] text-pink-400 text-center bg-pink-900/10 rounded`}
                  style={{ writingMode: compactMode ? 'vertical-rl' : 'horizontal-tb', transform: compactMode ? 'none' : 'rotate(-45deg)', transformOrigin: 'center' }}
                  title={`${dv.name}${dv.description ? ' - ' + dv.description : ''}`}
                >
                  {dv.name}
                </div>
              ))}
            </div>

            {/* Data Rows */}
            {filteredCVs.map((cv) => {
              const actualCvIdx = model.variables.cvs.indexOf(cv);
              return (
                <div key={cv.name} className="flex gap-1 mb-1">
                  <div 
                    className={`${compactMode ? 'w-24' : 'w-32'} flex-shrink-0 px-2 py-3 font-bold text-[10px] text-indigo-400 flex items-center bg-indigo-900/10 rounded`}
                    title={`${cv.name}${cv.description ? ' - ' + cv.description : ''}`}
                  >
                    {cv.name}
                  </div>
                  
                  {/* MV Response Cells */}
                  {filteredMVs.map((mv) => {
                    const actualMvIdx = model.variables.mvs.indexOf(mv);
                    const coeffs = getStepResponse(actualCvIdx, actualMvIdx, 'MV');
                    const hasResponse = hasData(coeffs);
                    const chartData = coeffs.map((gain, idx) => ({
                      time: idx * model.tuning.sample_time,
                      gain
                    }));
                    const finalGain = coeffs.length > 0 ? coeffs[coeffs.length - 1] : 0;
                    const isHovered = hoveredCell?.cvIdx === actualCvIdx && hoveredCell?.inputIdx === actualMvIdx && hoveredCell?.inputType === 'MV';
                    
                    // Get tau for tooltip
                    const tau = model.physics?.tau?.[actualCvIdx]?.[actualMvIdx];
                    
                    return (
                      <div 
                        key={mv.name}
                        className={`${cellWidth} flex-shrink-0 p-1 ${isHovered ? 'ring-2 ring-amber-500' : ''}`}
                        onMouseEnter={() => setHoveredCell({ cvIdx: actualCvIdx, inputIdx: actualMvIdx, inputType: 'MV' })}
                        onMouseLeave={() => setHoveredCell(null)}
                      >
                        <div
                          onClick={() => hasResponse && setSelectedCell({ 
                            cvIdx: actualCvIdx, 
                            inputIdx: actualMvIdx, 
                            inputType: 'MV',
                            cvName: cv.name,
                            inputName: mv.name,
                            chartData
                          })}
                          className={`${cellHeight} rounded border relative ${hasResponse ? 'border-amber-600/30 bg-slate-800/50 cursor-pointer hover:border-amber-500 hover:shadow-lg' : 'border-slate-700/20 bg-slate-900/20'} transition`}
                          title={hasResponse ? `${cv.name} ← ${mv.name}\nGain: ${finalGain.toFixed(4)}${tau ? `\nτ: ${tau.toFixed(1)}s` : ''}` : 'No data'}
                        >
                          {hasResponse ? (
                            <>
                              <ResponsiveContainer width="100%" height="100%">
                                <LineChart data={chartData} margin={{top: 5, right: 5, bottom: 5, left: 5}}>
                                  <CartesianGrid strokeDasharray="2 2" stroke="#334155" opacity={0.3} />
                                  <YAxis domain={['auto', 'auto']} hide />
                                  <Line type="monotone" dataKey="gain" stroke="#fbbf24" strokeWidth={1.5} dot={false} />
                                </LineChart>
                              </ResponsiveContainer>
                              {/* Final gain overlay */}
                              <div className="absolute bottom-1 right-1 bg-slate-950/80 px-1 py-0.5 rounded text-[8px] font-mono font-bold text-amber-400 border border-amber-600/30">
                                K={Math.abs(finalGain) < 0.01 ? finalGain.toExponential(1) : finalGain.toFixed(3)}
                              </div>
                            </>
                          ) : (
                            <div className="flex items-center justify-center h-full text-xs text-slate-600">No data</div>
                          )}
                        </div>
                      </div>
                    );
                  })}

                  {/* DV Response Cells */}
                  {filteredDVs.map((dv) => {
                    const actualDvIdx = model.variables.dvs?.indexOf(dv) ?? 0;
                    const coeffs = getStepResponse(actualCvIdx, actualDvIdx, 'DV');
                    const hasResponse = hasData(coeffs);
                    const chartData = coeffs.map((gain, idx) => ({
                      time: idx * model.tuning.sample_time,
                      gain
                    }));
                    const finalGain = coeffs.length > 0 ? coeffs[coeffs.length - 1] : 0;
                    const isHovered = hoveredCell?.cvIdx === actualCvIdx && hoveredCell?.inputIdx === actualDvIdx && hoveredCell?.inputType === 'DV';
                    
                    const tau = model.physics?.tau_dv?.[actualCvIdx]?.[actualDvIdx];
                    
                    return (
                      <div 
                        key={dv.name}
                        className={`${cellWidth} flex-shrink-0 p-1 ${isHovered ? 'ring-2 ring-pink-500' : ''}`}
                        onMouseEnter={() => setHoveredCell({ cvIdx: actualCvIdx, inputIdx: actualDvIdx, inputType: 'DV' })}
                        onMouseLeave={() => setHoveredCell(null)}
                      >
                        <div
                          onClick={() => hasResponse && setSelectedCell({ 
                            cvIdx: actualCvIdx, 
                            inputIdx: actualDvIdx, 
                            inputType: 'DV',
                            cvName: cv.name,
                            inputName: dv.name,
                            chartData
                          })}
                          className={`${cellHeight} rounded border relative ${hasResponse ? 'border-pink-600/30 bg-slate-800/50 cursor-pointer hover:border-pink-500 hover:shadow-lg' : 'border-slate-700/20 bg-slate-900/20'} transition`}
                          title={hasResponse ? `${cv.name} ← ${dv.name}\nGain: ${finalGain.toFixed(4)}${tau ? `\nτ: ${tau.toFixed(1)}s` : ''}` : 'No data'}
                        >
                          {hasResponse ? (
                            <>
                              <ResponsiveContainer width="100%" height="100%">
                                <LineChart data={chartData} margin={{top: 5, right: 5, bottom: 5, left: 5}}>
                                  <CartesianGrid strokeDasharray="2 2" stroke="#334155" opacity={0.3} />
                                  <YAxis domain={['auto', 'auto']} hide />
                                  <Line type="monotone" dataKey="gain" stroke="#ec4899" strokeWidth={1.5} dot={false} />
                                </LineChart>
                              </ResponsiveContainer>
                              <div className="absolute bottom-1 right-1 bg-slate-950/80 px-1 py-0.5 rounded text-[8px] font-mono font-bold text-pink-400 border border-pink-600/30">
                                K={Math.abs(finalGain) < 0.01 ? finalGain.toExponential(1) : finalGain.toFixed(3)}
                              </div>
                            </>
                          ) : (
                            <div className="flex items-center justify-center h-full text-xs text-slate-600">No data</div>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {/* Detail Modal */}
      {selectedCell && renderInteractionDetail()}

    </div>
  );
};
