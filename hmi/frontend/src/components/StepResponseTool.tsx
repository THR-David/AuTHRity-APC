import React, { useState } from 'react';
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer, ReferenceLine, ReferenceArea } from 'recharts';
import { Download } from 'lucide-react';
import { apiFetch } from '../lib/api';

interface TimeSeriesPoint {
  timestamp: string;
  value: number;
}

interface NodeHistory {
  node_id: string;
  tag: string;
  field: string;
  data: TimeSeriesPoint[];
}

interface StepResponseToolProps {
  cvNames: string[];
  mvNames: string[];
  dvNames: string[];
  sampleTime?: number;  // From tuning tab (seconds)
  tssMin?: number;      // Time to steady state (minutes)
}

export default function StepResponseTool({ cvNames, mvNames, dvNames, sampleTime = 20, tssMin = 15 }: StepResponseToolProps) {
  const [startTime, setStartTime] = useState('');
  const [endTime, setEndTime] = useState('');
  const [selectedInput, setSelectedInput] = useState(''); // Either MV or DV
  const [selectedCVs, setSelectedCVs] = useState<string[]>([]);
  const [historicalData, setHistoricalData] = useState<NodeHistory[]>([]);
  const [loading, setLoading] = useState(false);
  const [stepTime, setStepTime] = useState('');
  const [baselineDuration, setBaselineDuration] = useState(300); // 5 minutes
  const [calculatedResponse, setCalculatedResponse] = useState<any>(null);
  const [baselineTruncated, setBaselineTruncated] = useState(false);

  // Calculate response duration from time to steady state
  const responseDuration = tssMin * 60; // Convert minutes to seconds

  const loadHistoricalData = async () => {
    if (!startTime || !endTime) {
      alert('Please select start and end time');
      return;
    }

    if (!selectedInput || selectedCVs.length === 0) {
      alert('Please select an input variable (MV or DV) and at least one CV');
      return;
    }

    setLoading(true);
    try {
      // Build tags list: Input (MV:OP or DV:PV) and CVs:PV
      const inputNode = mvNames.includes(selectedInput) ? `${selectedInput}:OP` : `${selectedInput}:PV`;
      const tags = [
        inputNode,
        ...selectedCVs.map(cv => `${cv}:PV`)
      ].join(',');

      const params = new URLSearchParams({
        tags: tags,
        start: new Date(startTime).toISOString(),
        end: new Date(endTime).toISOString()
      });

      const response = await apiFetch(`/api/trends?${params}`);
      if (response.ok) {
        const data = await response.json();
        
        // Transform to NodeHistory format
        const nodeHistories: NodeHistory[] = [];
        const groupedByNode: Record<string, TimeSeriesPoint[]> = {};
        
        data.forEach((point: any) => {
          const nodeId = `${point.tag}:${point.field}`;
          if (!groupedByNode[nodeId]) {
            groupedByNode[nodeId] = [];
          }
          groupedByNode[nodeId].push({
            timestamp: point.timestamp,
            value: point.value
          });
        });
        
        Object.entries(groupedByNode).forEach(([nodeId, points]) => {
          const [tag, field] = nodeId.split(':');
          nodeHistories.push({
            node_id: nodeId,
            tag: tag,
            field: field,
            data: points
          });
        });
        
        setHistoricalData(nodeHistories);
        
        // Auto-detect step time (find largest input change)
        autoDetectStep(nodeHistories);
      } else {
        alert('Failed to load data: ' + await response.text());
      }
    } catch (error) {
      console.error('Failed to load historical data:', error);
      alert('Error loading data');
    } finally {
      setLoading(false);
    }
  };

  const autoDetectStep = (data: NodeHistory[]) => {
    // Find input variable data (MV:OP or DV:PV)
    const inputNode = mvNames.includes(selectedInput) ? `${selectedInput}:OP` : `${selectedInput}:PV`;
    const inputData = data.find(d => d.node_id === inputNode);
    if (!inputData || inputData.data.length < 2) return;

    // Find largest change between consecutive points
    let maxChange = 0;
    let stepIdx = 0;

    for (let i = 1; i < inputData.data.length; i++) {
      const change = Math.abs(inputData.data[i].value - inputData.data[i - 1].value);
      if (change > maxChange) {
        maxChange = change;
        stepIdx = i;
      }
    }

    if (maxChange > 0.5) {  // Threshold for detecting a step
      setStepTime(inputData.data[stepIdx].timestamp);
    }
  };

  const calculateStepResponse = async () => {
    if (!stepTime) {
      alert('Please set the step time (or use auto-detect)');
      return;
    }

    const inputNode = mvNames.includes(selectedInput) ? `${selectedInput}:OP` : `${selectedInput}:PV`;
    const inputData = historicalData.find(d => d.node_id === inputNode);
    const cvDataList = selectedCVs.map(cv => 
      historicalData.find(d => d.node_id === `${cv}:PV`)
    );

    if (!inputData || cvDataList.some(cv => !cv)) {
      alert('Missing data for calculation');
      return;
    }

    try {
      const request = {
        mv_node: inputNode,
        cv_nodes: selectedCVs.map(cv => `${cv}:PV`),
        mv_data: inputData.data.map(p => [p.timestamp, p.value]),
        cv_data: cvDataList.map(cv => cv!.data.map(p => [p.timestamp, p.value])),
        step_time: stepTime,
        baseline_duration: baselineDuration,
        response_duration: responseDuration,
        sample_time: sampleTime,
      };

      const response = await apiFetch('/api/stepresponse/calculate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(request),
      }, true);

      if (response.ok) {
        const result = await response.json();
        setCalculatedResponse(result);
      } else {
        alert('Calculation failed: ' + await response.text());
      }
    } catch (error) {
      console.error('Failed to calculate step response:', error);
      alert('Calculation error');
    }
  };

  const exportToJSON = () => {
    if (!calculatedResponse) {
      alert('No calculated response to export');
      return;
    }

    // Format for model loader
    const inputType = mvNames.includes(selectedInput) ? 'MV' : 'DV';
    const modelData = {
      timestamp: new Date().toISOString(),
      input_tag: selectedInput,
      input_type: inputType,
      step_size: calculatedResponse.step_size,
      step_time: calculatedResponse.step_time,
      responses: calculatedResponse.responses.map((resp: any) => ({
        cv_tag: resp.cv_node.split(':')[0],
        baseline: resp.cv_baseline,
        coefficients: resp.coefficients,
      })),
    };

    const blob = new Blob([JSON.stringify(modelData, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').split('T')[0];
    a.download = `model_${inputType}_${selectedInput}_${timestamp}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  // Prepare chart data - properly merge all timestamps
  const chartData = historicalData.length > 0 ? (() => {
    // Collect all unique timestamps across all nodes
    const grouped: Record<string, any> = {};
    
    historicalData.forEach(node => {
      node.data.forEach(point => {
        const key = point.timestamp;
        if (!grouped[key]) {
          grouped[key] = {
            timestamp: new Date(point.timestamp).getTime(), // Unix milliseconds for x-axis
            _sortKey: new Date(point.timestamp).getTime()
          };
        }
        grouped[key][node.node_id] = point.value;
      });
    });
    
    // Sort by timestamp and remove sort key
    return Object.values(grouped)
      .sort((a, b) => a._sortKey - b._sortKey)
      .map(({ _sortKey, ...rest }) => rest);
  })() : [];

  const toggleCV = (cv: string) => {
    setSelectedCVs(prev => 
      prev.includes(cv) ? prev.filter(c => c !== cv) : [...prev, cv]
    );
  };

  // Validate baseline window whenever stepTime or baselineDuration changes
  React.useEffect(() => {
    if (stepTime && startTime) {
      const stepMs = new Date(stepTime).getTime();
      const startMs = new Date(startTime).getTime();
      const baselineStartMs = stepMs - (baselineDuration * 1000);
      setBaselineTruncated(baselineStartMs < startMs);
    } else {
      setBaselineTruncated(false);
    }
  }, [stepTime, startTime, baselineDuration]);

  return (
    <div className="p-6 space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold text-slate-100">Model Generator</h1>
        <div className="text-xs text-slate-500">
          {mvNames.length} MVs · {cvNames.length} CVs · {dvNames.length} DVs
        </div>
      </div>

      {/* Step 1: Time Range & Variables */}
      <div className="bg-slate-800/50 rounded-lg p-5 space-y-4">
        <div className="grid grid-cols-4 gap-3">
          {/* Time Range */}
          <div className="col-span-2">
            <label className="block text-xs font-medium text-slate-400 mb-1.5">Time Range</label>
            <div className="grid grid-cols-2 gap-2">
              <input
                type="datetime-local"
                value={startTime}
                onChange={(e) => {
                  const newStart = e.target.value;
                  setStartTime(newStart);
                  // Smart autofill: Set endTime to 1 hour later if not already set
                  if (newStart && !endTime) {
                    const start = new Date(newStart);
                    const end = new Date(start.getTime() + 60 * 60 * 1000); // +1 hour
                    setEndTime(end.toISOString().slice(0, 16));
                  }
                }}
                className="w-full bg-slate-700/50 text-slate-100 rounded px-2.5 py-1.5 text-sm"
              />
              <input
                type="datetime-local"
                value={endTime}
                onChange={(e) => setEndTime(e.target.value)}
                className="w-full bg-slate-700/50 text-slate-100 rounded px-2.5 py-1.5 text-sm"
              />
            </div>
          </div>

          {/* Input Variable */}
          <div>
            <label className="block text-xs font-medium text-slate-400 mb-1.5">Input (MV/DV)</label>
            <select
              value={selectedInput}
              onChange={(e) => setSelectedInput(e.target.value)}
              className="w-full bg-slate-700/50 text-slate-100 rounded px-2.5 py-1.5 text-sm"
            >
              <option value="">Select...</option>
              <optgroup label="MVs">
                {mvNames.map(name => <option key={name} value={name}>{name}</option>)}
              </optgroup>
              <optgroup label="DVs">
                {dvNames.map(name => <option key={name} value={name}>{name}</option>)}
              </optgroup>
            </select>
          </div>

          {/* Output Variables */}
          <div>
            <label className="block text-xs font-medium text-slate-400 mb-1.5">Outputs (CVs)</label>
            <div className="bg-slate-700/50 rounded px-2.5 py-1.5 max-h-[34px] overflow-y-auto text-sm">
              {cvNames.length > 0 ? (
                <div className="flex flex-wrap gap-1">
                  {cvNames.map(name => (
                    <label key={name} className="flex items-center gap-1 text-slate-300 hover:text-slate-100 cursor-pointer text-xs">
                      <input
                        type="checkbox"
                        checked={selectedCVs.includes(name)}
                        onChange={() => toggleCV(name)}
                        className="w-3 h-3"
                      />
                      <span>{name}</span>
                    </label>
                  ))}
                </div>
              ) : (
                <span className="text-slate-500 text-xs">No CVs</span>
              )}
            </div>
          </div>
        </div>

        <button
          onClick={loadHistoricalData}
          disabled={loading}
          className="w-full bg-indigo-600 hover:bg-indigo-700 disabled:bg-slate-700 text-white font-medium py-2 rounded text-sm transition-colors"
        >
          {loading ? 'Loading...' : 'Load Data'}
        </button>
      </div>

      {/* Chart Section */}
      {historicalData.length > 0 && (
        <div className="bg-slate-800/50 rounded-lg p-5 space-y-3">
          <div className="text-xs text-slate-400 mb-2 flex items-center gap-2">
            <span>💡 <strong>Click on the chart</strong> to set the Step Time</span>
            {baselineTruncated && (
              <span className="ml-auto text-amber-400 font-medium">⚠️ Warning: Baseline window is truncated by the start of data</span>
            )}
          </div>
          <ResponsiveContainer width="100%" height={350}>
            <LineChart 
              data={chartData}
              onClick={(e) => {
                if (e && e.activeLabel) {
                  const clickedTime = new Date(e.activeLabel).toISOString();
                  setStepTime(clickedTime);
                }
              }}
              style={{ cursor: 'crosshair' }}
            >
              <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
              
              {/* Visual Calculation Windows */}
              {stepTime && (
                <>
                  {/* Baseline Area */}
                  <ReferenceArea
                    x1={new Date(stepTime).getTime() - (baselineDuration * 1000)}
                    x2={new Date(stepTime).getTime()}
                    fill="#64748b"
                    fillOpacity={0.1}
                    label={{ 
                      value: 'Baseline', 
                      position: 'insideTopLeft', 
                      fill: '#64748b', 
                      fontSize: 10,
                      fontWeight: 600,
                      offset: 5
                    }}
                  />
                  {/* Response Area */}
                  <ReferenceArea
                    x1={new Date(stepTime).getTime()}
                    x2={new Date(stepTime).getTime() + (responseDuration * 1000)}
                    fill="#3b82f6"
                    fillOpacity={0.1}
                    label={{ 
                      value: 'Response', 
                      position: 'insideTopRight', 
                      fill: '#3b82f6', 
                      fontSize: 10,
                      fontWeight: 600,
                      offset: 5
                    }}
                  />
                </>
              )}
              <XAxis 
                dataKey="timestamp" 
                type="number"
                domain={['dataMin', 'dataMax']}
                tickFormatter={(unixTime) => new Date(unixTime).toLocaleTimeString()}
                stroke="#64748b"
                tick={{ fontSize: 11 }}
              />
              <YAxis 
                stroke="#64748b"
                tick={{ fontSize: 11 }}
              />
              <Tooltip 
                contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '6px', fontSize: '12px' }}
                labelStyle={{ color: '#cbd5e1' }}
                labelFormatter={(unixTime) => new Date(unixTime).toLocaleString()}
                isAnimationActive={false}
              />
              <Legend wrapperStyle={{ fontSize: '11px' }} />
              
              {stepTime && (
                <ReferenceLine 
                  x={new Date(stepTime).getTime()} 
                  stroke="#fbbf24" 
                  strokeWidth={2}
                  strokeDasharray="5 5"
                  label={{ 
                    value: 'Step', 
                    position: 'top', 
                    fill: '#fbbf24', 
                    fontSize: 11,
                    fontWeight: 600
                  }}
                />
              )}
              
              {selectedInput && (
                <Line
                  type={mvNames.includes(selectedInput) ? "stepAfter" : "monotone"}
                  dataKey={mvNames.includes(selectedInput) ? `${selectedInput}:OP` : `${selectedInput}:PV`}
                  stroke="#ef4444"
                  strokeWidth={2}
                  dot={false}
                  isAnimationActive={false}
                  connectNulls={true}
                  name={selectedInput}
                />
              )}
              
              {selectedCVs.map((cv, idx) => (
                <Line
                  key={cv}
                  type="monotone"
                  dataKey={`${cv}:PV`}
                  stroke={['#3b82f6', '#10b981', '#f59e0b', '#8b5cf6'][idx % 4]}
                  strokeWidth={2}
                  dot={false}
                  isAnimationActive={false}
                  connectNulls={true}
                  name={cv}
                />
              ))}
            </LineChart>
          </ResponsiveContainer>

          <div className="grid grid-cols-3 gap-3 pt-2">
            <div>
              <label className="block text-xs font-medium text-slate-400 mb-1.5">Step Time</label>
              <input
                type="datetime-local"
                step="1"
                value={stepTime ? new Date(stepTime).toISOString().slice(0, 19) : ''}
                onChange={(e) => {
                  if (e.target.value) {
                    setStepTime(new Date(e.target.value).toISOString());
                  }
                }}
                className="w-full bg-slate-700/50 text-slate-100 rounded px-2.5 py-1.5 text-sm"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-slate-400 mb-1.5">Baseline (s)</label>
              <input
                type="number"
                value={baselineDuration}
                onChange={(e) => setBaselineDuration(Number(e.target.value))}
                className="w-full bg-slate-700/50 text-slate-100 rounded px-2.5 py-1.5 text-sm"
                min="10"
                step="10"
              />
            </div>
            <div className="flex items-end">
              <button
                onClick={calculateStepResponse}
                className="w-full bg-emerald-600 hover:bg-emerald-700 text-white font-medium py-1.5 rounded text-sm transition-colors"
              >
                Calculate
              </button>
            </div>
          </div>

          <div className="grid grid-cols-3 gap-2 text-xs text-slate-400 bg-slate-900/30 rounded p-2.5">
            <div>Sample Time: <span className="text-slate-200 font-medium">{sampleTime}s</span></div>
            <div>Response: <span className="text-slate-200 font-medium">{tssMin}min</span></div>
            <div>Points: <span className="text-slate-200 font-medium">{Math.ceil(responseDuration / sampleTime)}</span></div>
          </div>
        </div>
      )}

      {/* Results Section */}
      {calculatedResponse && (
        <div className="bg-slate-800/50 rounded-lg p-5 space-y-4">
          {/* Fit Quality Chart */}
          <div className="bg-slate-900/30 rounded p-4">
            <div className="text-xs font-medium text-slate-400 mb-3">Model Fit Quality</div>
            <ResponsiveContainer width="100%" height={280}>
              <LineChart>
                <CartesianGrid strokeDasharray="3 3" stroke="#334155" />
                <XAxis 
                  dataKey="time" 
                  type="number"
                  domain={[0, responseDuration]}
                  label={{ value: 'Time (s)', position: 'insideBottom', offset: -5, style: { fontSize: 10 } }}
                  stroke="#64748b"
                  tick={{ fontSize: 10 }}
                />
                <YAxis 
                  label={{ value: 'Gain', angle: -90, position: 'insideLeft', style: { fontSize: 10 } }}
                  stroke="#64748b"
                  tick={{ fontSize: 10 }}
                />
                <Tooltip 
                  contentStyle={{ backgroundColor: '#1e293b', border: '1px solid #334155', borderRadius: '6px', fontSize: '11px' }}
                  labelFormatter={(val) => `${Number(val).toFixed(1)}s`}
                />
                <Legend wrapperStyle={{ fontSize: '10px' }} />
                {calculatedResponse.responses.map((resp: any, idx: number) => {
                  const cvName = resp.cv_node.split(':')[0];
                  const colors = ['#3b82f6', '#10b981', '#f59e0b', '#ef4444'];
                  const color = colors[idx % colors.length];
                  
                  // Raw data points
                  const rawData = resp.raw_response.map(([time, gain]: [number, number]) => ({
                    time,
                    [`${cvName}_raw`]: gain
                  }));
                  
                  // Fitted points
                  const fittedData = resp.fitted_response.map(([time, gain]: [number, number]) => ({
                    time,
                    [`${cvName}_fitted`]: gain
                  }));
                  
                  return (
                    <React.Fragment key={cvName}>
                      <Line 
                        data={rawData}
                        type="monotone" 
                        dataKey={`${cvName}_raw`}
                        stroke={color}
                        strokeWidth={1}
                        dot={false}
                        name={`${cvName} raw`}
                        strokeOpacity={0.3}
                      />
                      <Line 
                        data={fittedData}
                        type="monotone" 
                        dataKey={`${cvName}_fitted`}
                        stroke={color}
                        strokeWidth={2}
                        dot={{ r: 3 }}
                        name={`${cvName} fit`}
                      />
                    </React.Fragment>
                  );
                })}
              </LineChart>
            </ResponsiveContainer>
          </div>

          {/* Results Summary */}
          <div className="grid grid-cols-4 gap-3 text-xs">
            <div className="bg-slate-900/30 rounded p-2.5">
              <div className="text-slate-500">MV Baseline</div>
              <div className="text-lg font-semibold text-slate-100 mt-1">
                {calculatedResponse.mv_baseline.toFixed(2)}
              </div>
            </div>
            <div className="bg-slate-900/30 rounded p-2.5">
              <div className="text-slate-500">Step Size</div>
              <div className="text-lg font-semibold text-slate-100 mt-1">
                {calculatedResponse.step_size.toFixed(2)}
              </div>
            </div>
            <div className="bg-slate-900/30 rounded p-2.5 col-span-2">
              <div className="text-slate-500">CV Responses</div>
              <div className="text-lg font-semibold text-slate-100 mt-1">
                {calculatedResponse.responses.length} × {calculatedResponse.responses[0]?.coefficients.length || 0} pts
              </div>
            </div>
          </div>

          {/* CV Details */}
          <div className="space-y-1.5">
            {calculatedResponse.responses.map((resp: any) => (
              <div key={resp.cv_node} className="bg-slate-900/30 rounded px-3 py-2 flex items-center justify-between text-xs">
                <span className="text-slate-300 font-medium">{resp.cv_node}</span>
                <span className="text-slate-500">
                  Baseline: <span className="text-slate-300">{resp.cv_baseline.toFixed(2)}</span>
                </span>
              </div>
            ))}
          </div>

          <button
            onClick={exportToJSON}
            className="w-full bg-violet-600 hover:bg-violet-700 text-white font-medium py-2.5 rounded flex items-center justify-center gap-2 text-sm transition-colors"
          >
            <Download className="w-4 h-4" />
            Export Model JSON
          </button>
        </div>
      )}
    </div>
  );
}
