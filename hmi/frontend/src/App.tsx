import { useEffect, useState, useRef } from 'react';
import { ProcessTable } from './components/ProcessTable';
import { Sidebar } from './components/Sidebar';
import { ToggleSwitch } from './components/ToggleSwitch';
import { TabNavigation } from './components/TabNavigation'; 
import { ModelGenerator } from './components/ModelGenerator';
import { PredictionGraph } from './components/PredictionGraph';
import { NextRunDisplay } from './components/NextRunDisplay';
import { TrendsTab } from './components/TrendsTab';
import { TuningTab } from './components/TuningTab';
import { ManageTab } from './components/ManageTab';
import { ModelsTab } from './components/ModelsTab';
import { PlantOverview } from './components/PlantOverview';
import { SettingsView } from './components/SettingsView';
import { ControllerDiagnostics } from './components/ControllerDiagnostics';
import { useTagStore } from './store/tagStore';
import { apiChangePassword, apiLogin, apiLogout, apiMe, hasRoleAtLeast, type UserRole } from './lib/api';

// --- TYPES ---
type OpcUpdate = {
  node_id: string;
  value: number | string | boolean | number[];
  timestamp: string;
  status: number;
};

// Represents a node from the YAML file (Flat Structure)
type NodeSpec = {
  node_id: string;
  node_class: string;
  description?: string;
  initial_value?: any; 
};

type ModelConfig = {
  nodes: NodeSpec[];
};

function App() {
    const [authReady, setAuthReady] = useState(false);
    const [authenticated, setAuthenticated] = useState(false);
    const [username, setUsername] = useState('');
    const [role, setRole] = useState<UserRole>('viewer');
    const [loginUser, setLoginUser] = useState('');
    const [loginPassword, setLoginPassword] = useState('');
    const [authError, setAuthError] = useState('');
    const [forcePasswordChange, setForcePasswordChange] = useState(false);
    const [showPasswordDialog, setShowPasswordDialog] = useState(false);
    const [currentPassword, setCurrentPassword] = useState('');
    const [newPassword, setNewPassword] = useState('');
    const [passwordStatus, setPasswordStatus] = useState('');

  const [wsConnected, setWsConnected] = useState(false); 
  // DON'T subscribe to tags here - it causes re-render on every update!
  // Only import the update function
  const updateTag = useTagStore(state => state.updateTag);
  
  const [descriptions, setDescriptions] = useState<Record<string, string>>({});
  
  // These lists are populated by "Scanning" the YAML structure
  const [cvList, setCvList] = useState<string[]>([]);
  const [mvList, setMvList] = useState<string[]>([]);
  const [dvList, setDvList] = useState<string[]>([]);

  const [configLoaded, setConfigLoaded] = useState(false);
  const [calcOn, setCalcOn] = useState(false);
  const [controlEnable, setControlEnable] = useState(false);
  const [systemPrefix, setSystemPrefix] = useState("");
  
  // Default active model
  const [activeModel, setActiveModel] = useState("Plant Overview"); 
  const [activeTab, setActiveTab] = useState("Overview");  

  const [dataStale, setDataStale] = useState(false);
    const [controllerRunning, setControllerRunning] = useState<boolean | null>(null);
    const [controllerSummary, setControllerSummary] = useState<any | null>(null);

  const ws = useRef<WebSocket | null>(null);

    useEffect(() => {
        const bootstrapAuth = async () => {
            try {
                const me = await apiMe();
                if (me.authenticated && me.username && me.role) {
                    setAuthenticated(true);
                    setUsername(me.username);
                    setRole(me.role);
                    setForcePasswordChange(me.force_password_change);
                }
            } finally {
                setAuthReady(true);
            }
        };
        bootstrapAuth();
    }, []);

  // 1. CONFIG LOAD (YAML Structure Inference)
  useEffect(() => {
        if (!authenticated) return;
    if (activeModel === "Generator" || activeModel === "Plant Overview") {
        setSystemPrefix("");
        setConfigLoaded(true);
        return;
    }

    const controller = new AbortController();
    const selectedModel = activeModel;

    setConfigLoaded(false);
    setSystemPrefix("");
    setCvList([]);
    setMvList([]);
    setDvList([]);
    setDescriptions({});

    fetch(`/api/model?file=${selectedModel}`, { signal: controller.signal })
      .then(res => res.json())
      .then((data: ModelConfig) => {
        if (controller.signal.aborted) return;
        const nodes = data.nodes;
        
        // Find system prefix (e.g., "CSTR_DMC", "Debutanizer_DMC")
        const systemNode = nodes.find(n => n.node_id.endsWith(":ControlNodes"));
        const prefix = systemNode ? systemNode.node_id.replace(":ControlNodes", "") : "System";
        setSystemPrefix(prefix);
        console.log("🔧 System node prefix:", prefix);
        
        // Store prefix in tags so WebSocket handler can access it
        updateTag({
          node_id: "_systemPrefix",
          value: prefix,
          timestamp: new Date().toISOString(),
          status: 1
        });
        
        // 1. FILTER: Exclude "System" AND "ControlNodes"
        const potentialTags = nodes.filter(n => 
            n.node_class === "Object" && 
            !n.node_id.endsWith(":ControlNodes")
        );
        
        const allNodeIds = new Set(nodes.map(n => n.node_id));
        
        const detectedCVs: string[] = [];
        const detectedMVs: string[] = [];
        const detectedDVs: string[] = []; // <--- 1. New Array
        const descriptionsMap: Record<string, string> = {};
        
        potentialTags.forEach(tag => {
            // Store description if available
            if (tag.description) {
                descriptionsMap[tag.node_id] = tag.description;
            }
            
            // CVs have a Prediction array
            const hasPrediction = allNodeIds.has(`${tag.node_id}:Prediction`);
            // MVs have a FuturePlan array
            const hasFuturePlan = allNodeIds.has(`${tag.node_id}:FuturePlan`);
            
            if (hasPrediction) {
                detectedCVs.push(tag.node_id);
            } else if (hasFuturePlan) {
                detectedMVs.push(tag.node_id);
            } else {
                // <--- 2. UPDATED LOGIC
                // If it's an Object but has no Prediction or FuturePlan, 
                // we treat it as a Disturbance Variable (DV).
                detectedDVs.push(tag.node_id);
            }
        });

        setCvList(detectedCVs);
        setMvList(detectedMVs);
        setDvList(detectedDVs); // <--- 3. Set State
        setDescriptions(descriptionsMap);
                setConfigLoaded(true);
      })
      .catch(err => {
                    if (controller.signal.aborted) return;
          console.error("Failed to load config:", err);
                    setSystemPrefix("");
          setConfigLoaded(true);
      });

        return () => {
            controller.abort();
        };
        }, [activeModel, authenticated]);

  // 2. WEBSOCKET
  useEffect(() => {
        if (!authenticated) return;
    // Persistent WebSocket connection - Initialize once
    if (ws.current) return;

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const host = window.location.host; 
    const url = `${protocol}//${host}/ws`;
    
    const socket = new WebSocket(url);
    ws.current = socket;

    socket.onopen = () => {
        setWsConnected(true);
        // Ask backend to refresh data immediately
        if (ws.current) {
            ws.current.send(JSON.stringify({ type: "REFRESH" }));
        }
    };

    socket.onclose = () => { 
        setWsConnected(false);
        ws.current = null;
    };
    
    socket.onmessage = (event) => {
      try {
        const msg: OpcUpdate = JSON.parse(event.data);
        updateTag(msg); // Use Zustand store update
        
        // IMPORTANT: Only update toggles if this is the active model's OperatingMode
        if (msg.node_id.endsWith(":OperatingMode")) {
             const msgPrefix = msg.node_id.replace(":OperatingMode", "");
             const currentPrefix = useTagStore.getState().tags["_systemPrefix"]?.value;
             if (currentPrefix && msgPrefix === currentPrefix) {
                 const mode = Number(msg.value);
                 // console.log(`🎛️  OperatingMode received: ${mode} from ${msg.node_id} (active model)`);
                 setCalcOn(mode >= 1);
                 setControlEnable(mode === 2);
             }
        }
      } catch (e) { console.error(e); }
    };
    
    return () => { 
        // In development (Strict Mode), effects run twice. 
        // We want to keep the connection if possible, or close it cleanly.
        socket.close();
        ws.current = null;
    };
    }, [authenticated]); // Dependency array empty -> Run once on mount

  // 3. SYNC TOGGLE STATES FROM OPC UA VALUES (Initial Load Only)
  useEffect(() => {
    if (!systemPrefix) return;
    
    const opModeNode = `${systemPrefix}:OperatingMode`;
    const opModeValue = useTagStore.getState().tags[opModeNode]?.value;
    
    if (opModeValue !== undefined) {
      const mode = Number(opModeValue);
      setCalcOn(mode >= 1);
      setControlEnable(mode === 2);
      console.log(`🔄 Synced toggles from OPC UA: mode=${mode}, calcOn=${mode >= 1}, controlEnable=${mode === 2}`);
    }
  }, [systemPrefix]); // Only run when systemPrefix changes (model switch)

  // 4. HEARTBEAT CHECK
  useEffect(() => {
    if (!systemPrefix) return;
    
    // Check heartbeat every 5 seconds
    const interval = setInterval(() => {
        const heartbeatNode = `${systemPrefix}:Heartbeat`;
        const tag = useTagStore.getState().tags[heartbeatNode];
        
        if (tag) {
            const lastTime = new Date(tag.timestamp).getTime();
            const now = Date.now();
            // If data is older than 10 seconds (allow 2 missed cycles), consider it stale
            if (now - lastTime > 10000) {
                setDataStale(true);
            } else {
                setDataStale(false);
            }
        } else {
            setDataStale(true);
        }
    }, 5000);

    return () => clearInterval(interval);
  }, [systemPrefix]);

    // 5. CONTROLLER RUNNING STATUS (from /api/prox/controllers)
    useEffect(() => {
        if (!authenticated) return;

        const isControllerView = activeModel !== "Generator" && activeModel !== "Plant Overview" && activeModel !== "Settings";
        if (!isControllerView) {
            setControllerRunning(null);
            setControllerSummary(null);
            return;
        }

        const parseIsRunning = (state: any): boolean => {
            if (typeof state === 'string') {
                return state.toLowerCase() === 'running';
            }
            if (state && typeof state === 'object') {
                return Object.prototype.hasOwnProperty.call(state, 'Running');
            }
            return false;
        };

        let isCancelled = false;

        const refreshControllerState = async () => {
            try {
                const res = await fetch('/api/prox/controllers');
                if (!res.ok) return;

                const controllers = await res.json();
                if (isCancelled || !Array.isArray(controllers)) return;

                const target = controllers.find((controller: any) =>
                    controller?.id === activeModel || (systemPrefix && controller?.id === systemPrefix)
                );

                if (!target) {
                    setControllerRunning(false);
                    setControllerSummary(null);
                    return;
                }

                setControllerRunning(parseIsRunning(target.state));
                setControllerSummary(target);
            } catch {
            }
        };

        refreshControllerState();
        const interval = setInterval(refreshControllerState, 5000);

        return () => {
            isCancelled = true;
            clearInterval(interval);
        };
    }, [authenticated, activeModel, systemPrefix]);

  const handleWrite = (nodeId: string, newValue: string | number | boolean) => {
        if (!hasRoleAtLeast(role, 'operator')) return;
    if (!ws.current || ws.current.readyState !== WebSocket.OPEN) return;
    
    let payload: any;
    if (typeof newValue === 'string') {
      payload = { type: "WRITE", nodeId: nodeId, value: newValue };
    } else if (typeof newValue === 'boolean') {
      payload = { type: "WRITE", nodeId: nodeId, value: newValue };
    } else {
      payload = { type: "WRITE", nodeId: nodeId, value: Number(newValue) };
    }
    
    ws.current.send(JSON.stringify(payload));
  };

  const getStatusColor = () => {
    if (!wsConnected) return "bg-red-500"; 

        if (systemPrefix) {
            if (controllerRunning === false) return "bg-red-500";
            if (controllerRunning === null) return "bg-amber-500 animate-pulse";

            const heartbeatNode = `${systemPrefix}:Heartbeat`;
            const tag = useTagStore.getState().tags[heartbeatNode];
            if (!tag) return "bg-amber-500 animate-pulse";

            const ageMs = Date.now() - new Date(tag.timestamp).getTime();
            if (ageMs > 10000 || dataStale) return "bg-amber-500 animate-pulse";
            return "bg-emerald-500";
        }

        if (dataStale) return "bg-amber-500 animate-pulse";
    const plcStatus = useTagStore.getState().tags["System:PlcConnection"]?.value;
    if (plcStatus === 1) return "bg-emerald-500"; 
    return "bg-amber-500 animate-pulse"; 
  };

  const getStatusText = () => {
      if (!wsConnected) return "Server Disconnected";

            if (systemPrefix) {
          if (controllerRunning === false) return "System Offline";
          if (controllerRunning === null) return "Checking controller...";

                    const heartbeatNode = `${systemPrefix}:Heartbeat`;
                    const tag = useTagStore.getState().tags[heartbeatNode];
          if (!tag) return "Data Stale (Heartbeat Lost)";

                    const ageMs = Date.now() - new Date(tag.timestamp).getTime();
                    if (ageMs > 10000 || dataStale) return "Data Stale (Heartbeat Lost)";
                    return "System Online";
            }

            if (dataStale) return "Data Stale (Heartbeat Lost)";
      const plcStatus = useTagStore.getState().tags["System:PlcConnection"]?.value;
      if (plcStatus === 1) return "System Online";
      return "Connecting to PLC...";
  };

  const handleToggleMode = (isCalc: boolean, isControl: boolean) => {
      if (!hasRoleAtLeast(role, 'operator')) return;
      if (!systemPrefix) return;
      let newMode = 0;
      if (isCalc && !isControl) newMode = 1;
      if (isCalc && isControl) newMode = 2;
      setCalcOn(isCalc);
      setControlEnable(isControl);
      const nodeId = `${systemPrefix}:OperatingMode`;
      console.log(`📩 Web Command: Write ${newMode} to ${nodeId}`);
      handleWrite(nodeId, newMode);
  };

    const canOperate = hasRoleAtLeast(role, 'operator');
    const canEngineer = hasRoleAtLeast(role, 'engineer');
    const canAdmin = role === 'admin';

    const tabs = canEngineer
        ? ["Overview", "Trends", "Models", "Tuning", "Manage", "Details"]
        : canOperate
            ? ["Overview", "Trends", "Models", "Tuning", "Details"]
            : ["Overview", "Trends", "Models", "Details"];

    useEffect(() => {
        if (!tabs.includes(activeTab)) {
            setActiveTab(tabs[0]);
        }
    }, [activeTab, tabs]);

    const onLogin = async (e: React.FormEvent) => {
        e.preventDefault();
        setAuthError('');
        try {
            const result = await apiLogin(loginUser.trim(), loginPassword);
            setAuthenticated(true);
            setUsername(result.username);
            setRole(result.role);
            setForcePasswordChange(result.force_password_change);
            setLoginPassword('');
        } catch (error: any) {
            setAuthError(error?.message || 'Login failed');
        }
    };

    const onLogout = async () => {
        try {
            await apiLogout();
        } finally {
            setAuthenticated(false);
            setUsername('');
            setRole('viewer');
            setForcePasswordChange(false);
            setShowPasswordDialog(false);
            if (ws.current) {
                ws.current.close();
                ws.current = null;
            }
        }
    };

    const openChangePassword = () => {
        setPasswordStatus('');
        setCurrentPassword('');
        setNewPassword('');
        setShowPasswordDialog(true);
    };

    const onChangePassword = async (e: React.FormEvent) => {
        e.preventDefault();
        setPasswordStatus('');
        if (!forcePasswordChange && !currentPassword.trim()) {
            setPasswordStatus('❌ Current password is required');
            return;
        }
        if (newPassword.trim().length < 8) {
            setPasswordStatus('❌ New password must be at least 8 characters');
            return;
        }

        try {
            await apiChangePassword(forcePasswordChange ? '' : currentPassword, newPassword.trim());
            const me = await apiMe();
            if (!me.authenticated || !me.username || !me.role) {
                setAuthenticated(false);
                setUsername('');
                setRole('viewer');
                setForcePasswordChange(false);
                setShowPasswordDialog(false);
                setPasswordStatus('✅ Password changed. Please sign in again.');
                return;
            }

            setAuthenticated(true);
            setUsername(me.username);
            setRole(me.role);
            setForcePasswordChange(me.force_password_change);
            setShowPasswordDialog(false);
            setCurrentPassword('');
            setNewPassword('');
            setPasswordStatus('✅ Password changed');
        } catch (error: any) {
            setPasswordStatus(error?.message || '❌ Password change failed');
        }
    };

    if (!authReady) {
        return <div className="h-screen bg-slate-950 flex items-center justify-center text-slate-400">Loading session...</div>;
    }

    if (!authenticated) {
        return (
            <div className="h-screen bg-slate-950 flex items-center justify-center text-slate-200">
                <form onSubmit={onLogin} className="w-full max-w-sm bg-slate-900 border border-slate-800 rounded-xl p-6 space-y-4">
                      <h1 className="text-xl font-bold text-white">AuTHRity Login</h1>
                    <input
                        value={loginUser}
                        onChange={(e) => setLoginUser(e.target.value)}
                        placeholder="Username"
                        className="w-full bg-slate-950 border border-slate-700 rounded px-3 py-2"
                    />
                    <input
                        type="password"
                        value={loginPassword}
                        onChange={(e) => setLoginPassword(e.target.value)}
                        placeholder="Password"
                        className="w-full bg-slate-950 border border-slate-700 rounded px-3 py-2"
                    />
                    {authError && <div className="text-rose-400 text-sm">{authError}</div>}
                    <button className="w-full bg-indigo-600 hover:bg-indigo-500 rounded py-2 font-bold">Sign in</button>
                </form>
            </div>
        );
    }

    if (!configLoaded && activeModel !== "Generator" && activeModel !== "Plant Overview") return <div className="h-screen bg-slate-900 flex items-center justify-center text-slate-500">Loading Configuration...</div>;

  return (
    <div className="flex h-screen bg-slate-950 font-mono text-slate-200 overflow-hidden">
      
            <Sidebar
                activeModel={activeModel}
                onSelect={setActiveModel}
                allowGenerator={canEngineer}
                allowSettings={canEngineer}
                username={username}
                role={role}
                onChangePassword={openChangePassword}
                onLogout={onLogout}
            />

      <div className="flex-1 flex flex-col h-screen overflow-hidden">
        
        {/* --- VIEW SWITCHER --- */}
        {activeModel === "Generator" ? (
            canEngineer ? <ModelGenerator /> : <div className="h-full flex items-center justify-center text-slate-500">Forbidden</div>
        ) : activeModel === "Settings" ? (
            canEngineer ? <SettingsView isAdmin={canAdmin} /> : <div className="h-full flex items-center justify-center text-slate-500">Forbidden</div>
        ) : activeModel === "Plant Overview" ? (
            <PlantOverview wsRef={ws} onSelectModel={setActiveModel} />
        ) : (
            <>
                <header className="h-16 bg-slate-900 border-b border-slate-800 flex items-center justify-between px-8 shadow-sm z-10 shrink-0">
                <div>
                    <h2 className="text-xl font-bold text-white tracking-tight">{activeModel}</h2>
                    <div className="flex items-center gap-2 text-xs text-slate-500 mt-1">
                        <span className={`w-2 h-2 rounded-full transition-colors duration-500 ${getStatusColor()}`}></span>
                        {getStatusText()}
                    </div>
                </div>
                </header>

                <div className="flex items-center justify-between border-b border-slate-800 bg-slate-900/50 px-8 shrink-0">
                    <TabNavigation 
                        tabs={tabs} 
                        activeTab={activeTab} 
                        onTabChange={setActiveTab} 
                    />

                    <div className="flex items-center gap-6 py-2">
                        <NextRunDisplay systemPrefix={systemPrefix} />
                        <div className="w-px h-8 bg-slate-700/50"></div>
                        <ToggleSwitch 
                            label="Calculator"  
                            enabled={calcOn} 
                            onToggle={(val) => {
                                if (!val) handleToggleMode(false, false);
                                else handleToggleMode(true, controlEnable);
                            }} 
                            colorOn="bg-blue-500"
                            disabled={!canOperate || !systemPrefix}
                        />
                        <div className="w-px h-8 bg-slate-700/50"></div>
                        <ToggleSwitch 
                            label="Control Action" 
                            enabled={controlEnable} 
                            onToggle={(val) => {
                                if (val) handleToggleMode(true, true);
                                else handleToggleMode(calcOn, false);
                            }} 
                            colorOn="bg-emerald-500"
                            disabled={!canOperate || !systemPrefix}
                        />
                    </div>
                </div>

                <main className="flex-1 overflow-y-auto p-8">
                    {activeTab === "Overview" && (
                        <div className="animate-in fade-in duration-300">
                            {/* CV TABLE */}
                            <ProcessTable 
                                title="Controlled Variables (CVs)" 
                                rowHeaders={cvList} 
                                descriptions={descriptions}
                                wsRef={ws} 
                                currentRole={role}
                                columns={[
                                    { header: "Description", suffix: "Description" },
                                    { header: "Status", suffix: "Status" },
                                    { header: "Low", suffix: "LowLimit" },
                                    { header: "PV", suffix: "PV" },
                                    { header: "Target", suffix: "Target" },
                                    { header: "Steady", suffix: "SteadyState" },
                                    { header: "High", suffix: "HighLimit" },
                                ]}
                            />
                            {/* --- ADDED: DV TABLE --- */}
                            {dvList.length > 0 && (
                                <ProcessTable 
                                    title="Disturbance Variables (DVs)" 
                                    rowHeaders={dvList} 
                                    descriptions={descriptions}
                                    wsRef={ws} 
                                    currentRole={role}
                                    columns={[
                                        { header: "Low",    suffix: "LowLimit" },
                                        { header: "PV",     suffix: "PV" },
                                        { header: "High",   suffix: "HighLimit" },
                                    ]}
                                />
                            )}

                            {/* MV TABLE */}
                            <ProcessTable 
                                title="Manipulated Variables (MVs)" 
                                rowHeaders={mvList} 
                                descriptions={descriptions}
                                wsRef={ws}
                                currentRole={role}
                                columns={[
                                    { header: "Description", suffix: "Description" },
                                    { header: "Status", suffix: "Status" },
                                    { header: "Low", suffix: "LowLimit" },
                                    { header: "PV", suffix: "PV" },
                                    { header: "SP", suffix: "SP" },
                                    { header: "Target", suffix: "Target" },
                                    { header: "Steady", suffix: "SteadyState" },
                                    { header: "High", suffix: "HighLimit" },
                                    { header: "Move", suffix: "LastMove" },
                                ]}
                            />

                            {/* PREDICTION GRAPH */}
                            <PredictionGraph 
                                cvList={cvList}
                                mvList={mvList}
                                sampleTime={20}
                            />
                        </div>
                    )}
                    {activeTab === "Trends" && (
                        <div className="animate-in fade-in duration-300">
                            <TrendsTab 
                                cvList={cvList}
                                mvList={mvList}
                                dvList={dvList}
                            />
                        </div>
                    )}
                    {activeTab === "Models" && (
                        <div className="animate-in fade-in duration-300">
                            <ModelsTab controllerId={systemPrefix} />
                        </div>
                    )}
                    {activeTab === "Tuning" && (
                        <div className="animate-in fade-in duration-300">
                            <TuningTab 
                                cvList={cvList}
                                mvList={mvList}
                                descriptions={descriptions}
                                wsRef={ws}
                                systemPrefix={systemPrefix}
                            />
                        </div>
                    )}
                    {activeTab === "Manage" && (
                        <div className="animate-in fade-in duration-300">
                            <ManageTab 
                                systemPrefix={systemPrefix}
                                wsRef={ws}
                                cvList={cvList}
                                mvList={mvList}
                                dvList={dvList}
                            />
                        </div>
                    )}
                    {activeTab === "Details" && (
                        <div className="h-full flex flex-col">
                            <ControllerDiagnostics
                                controllerId={systemPrefix || activeModel}
                                summary={controllerSummary}
                            />
                        </div>
                    )}
                    <div className="text-center text-xs text-slate-600 mt-8 mb-4">
                        AuTHRity Web HMI v0.4 • Powered by Rust & React
                    </div>
                </main>
            </>
        )}
      </div>

            {(forcePasswordChange || showPasswordDialog) && (
                <div className="fixed inset-0 bg-black/70 flex items-center justify-center z-50">
                    <form onSubmit={onChangePassword} className="w-full max-w-md bg-slate-900 border border-slate-700 rounded-xl p-6 space-y-4">
                        <h3 className="text-lg font-bold text-white">{forcePasswordChange ? 'Password Update Required' : 'Change Password'}</h3>
                        <p className="text-xs text-slate-400">
                            {forcePasswordChange
                                ? 'You must set a new password before continuing. Current password is not required.'
                                : 'Update your account password.'}
                        </p>
                        {!forcePasswordChange && (
                            <input
                                type="password"
                                value={currentPassword}
                                onChange={(e) => setCurrentPassword(e.target.value)}
                                placeholder="Current password"
                                className="w-full bg-slate-950 border border-slate-700 rounded px-3 py-2"
                            />
                        )}
                        <input
                            type="password"
                            value={newPassword}
                            onChange={(e) => setNewPassword(e.target.value)}
                            placeholder="New password"
                            className="w-full bg-slate-950 border border-slate-700 rounded px-3 py-2"
                        />
                        {passwordStatus && <div className="text-sm text-slate-300">{passwordStatus}</div>}
                        <div className="flex items-center gap-2">
                            {!forcePasswordChange && (
                                <button
                                    type="button"
                                    onClick={() => setShowPasswordDialog(false)}
                                    className="flex-1 bg-slate-800 hover:bg-slate-700 rounded py-2 font-bold"
                                >
                                    Cancel
                                </button>
                            )}
                            <button className="flex-1 bg-indigo-600 hover:bg-indigo-500 rounded py-2 font-bold">
                                Update Password
                            </button>
                        </div>
                    </form>
                </div>
            )}
    </div>
  );
}

export default App;