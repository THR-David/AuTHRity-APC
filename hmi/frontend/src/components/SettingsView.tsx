import React, { useState, useEffect } from 'react';
import { Plus, Trash2, Server, Network, Users, UserPlus, KeyRound, UserCog, ClipboardList, Download } from 'lucide-react';
import { apiFetch, type UserRole } from '../lib/api';
import { useTagStore } from '../store/tagStore';

export interface ServiceConfig {
    id: string;
    name: string;
    url: string;
    opc_endpoint?: string;
    security_policy?: string;
    security_mode?: string;
    auth_mode?: string;
}

interface HmiClientSettings {
    security_policy: string;
    security_mode: string;
    auth_mode: string;
    username_ref?: string;
    cert_ref?: string;
    username?: string;
    password?: string;
}

interface ControllerHostClientSettings {
    supervisor_id: string;
    security_policy: string;
    security_mode: string;
    auth_mode: string;
    username_ref?: string;
    cert_ref?: string;
    username?: string;
    password?: string;
}

interface InfrastructureConfig {
    supervisors: ServiceConfig[];
    opc_servers: ServiceConfig[];
    hmi_client?: HmiClientSettings;
    controller_host_clients?: ControllerHostClientSettings[];
}

interface OpcSecurityEndpoint {
    id: string;
    path: string;
    security_policy: string;
    security_mode: string;
    security_level: number;
    user_token_ids: string[];
}

interface OpcSecurityConfig {
    default_endpoint: string;
    endpoints: OpcSecurityEndpoint[];
}

interface OpcUserToken {
    id: string;
    user: string;
    has_password: boolean;
    x509?: string;
}

interface ManagedUser {
    id: number;
    username: string;
    role: UserRole;
    disabled: boolean;
    created_at: string;
}

interface AuditEvent {
    id: number;
    actor_username: string | null;
    actor_role: string | null;
    action: string;
    target: string | null;
    result: string;
    detail: string | null;
    created_at: string;
}

const roleOptions: UserRole[] = ['viewer', 'operator', 'engineer', 'admin'];
const securityPolicyOptions = ['None', 'Basic256Sha256'];
const securityModeOptions = ['None', 'Sign', 'SignAndEncrypt'];

const defaultHmiClientSettings: HmiClientSettings = {
    security_policy: 'Basic256Sha256',
    security_mode: 'SignAndEncrypt',
    auth_mode: 'Username',
    username_ref: 'hmi_client_default',
    cert_ref: undefined,
    username: undefined,
    password: undefined,
};

const defaultControllerHostClient = (supervisorId: string): ControllerHostClientSettings => ({
    supervisor_id: supervisorId,
    security_policy: defaultHmiClientSettings.security_policy,
    security_mode: defaultHmiClientSettings.security_mode,
    auth_mode: defaultHmiClientSettings.auth_mode,
    username_ref: `controller_host_${supervisorId}`,
    cert_ref: undefined,
    username: undefined,
    password: undefined,
});

interface SettingsViewProps {
    isAdmin: boolean;
}

export const SettingsView: React.FC<SettingsViewProps> = ({ isAdmin }) => {
    const [activeTab, setActiveTab] = useState<'Infrastructure' | 'Users' | 'Audit'>('Infrastructure');

    const [supervisors, setSupervisors] = useState<ServiceConfig[]>([]);
    const [opcServers, setOpcServers] = useState<ServiceConfig[]>([]);
    const [hmiClient, setHmiClient] = useState<HmiClientSettings>(defaultHmiClientSettings);
    const [controllerHostClients, setControllerHostClients] = useState<ControllerHostClientSettings[]>([]);
    const [infraStatus, setInfraStatus] = useState<string>('');
    const [activeOpcSecurityServerId, setActiveOpcSecurityServerId] = useState<string>('');
    const [opcSecurityConfig, setOpcSecurityConfig] = useState<OpcSecurityConfig | null>(null);
    const [opcTokens, setOpcTokens] = useState<OpcUserToken[]>([]);
    const [opcSecurityStatus, setOpcSecurityStatus] = useState<string>('');
    const [opcTokenDraft, setOpcTokenDraft] = useState({ user: '', pass: '', x509: '' });
    const [showRestartConfirm, setShowRestartConfirm] = useState(false);

    const [users, setUsers] = useState<ManagedUser[]>([]);
    const [usersStatus, setUsersStatus] = useState<string>('');
    const [newUsername, setNewUsername] = useState('');
    const [newPassword, setNewPassword] = useState('');
    const [newRole, setNewRole] = useState<UserRole>('viewer');
    const [resetPasswords, setResetPasswords] = useState<Record<number, string>>({});

    const [auditStatus, setAuditStatus] = useState('');
    const [auditEvents, setAuditEvents] = useState<AuditEvent[]>([]);
    const [auditActor, setAuditActor] = useState('');
    const [auditAction, setAuditAction] = useState('');
    const [auditResult, setAuditResult] = useState('');
    const [auditLimit, setAuditLimit] = useState(200);
    const opcConnection = useTagStore((state) => state.tags['System:PlcConnection']?.value);
    const opcAuthError = useTagStore((state) => state.tags['System:PlcAuthError']?.value);
    const opcLastError = useTagStore((state) => state.tags['System:PlcLastError']?.value);

    const visibleTabs: Array<'Infrastructure' | 'Users' | 'Audit'> = isAdmin
        ? ['Infrastructure', 'Users', 'Audit']
        : ['Infrastructure'];

    useEffect(() => {
        fetchSettings();
        if (isAdmin) {
            fetchUsers();
            fetchAudit();
        }
    }, [isAdmin]);

    useEffect(() => {
        if (!visibleTabs.includes(activeTab)) {
            setActiveTab('Infrastructure');
        }
    }, [activeTab, visibleTabs]);

    useEffect(() => {
        if (activeOpcSecurityServerId) {
            void fetchOpcSecurityData(activeOpcSecurityServerId);
        }
    }, [activeOpcSecurityServerId]);

    const fetchAudit = async () => {
        try {
            const params = new URLSearchParams();
            params.set('limit', String(auditLimit));
            if (auditActor.trim()) params.set('actor', auditActor.trim());
            if (auditAction.trim()) params.set('action', auditAction.trim());
            if (auditResult.trim()) params.set('result', auditResult.trim());

            const res = await apiFetch(`/api/admin/audit?${params.toString()}`);
            if (!res.ok) {
                setAuditStatus(`❌ Failed to load audit events: ${await res.text()}`);
                return;
            }

            const data: AuditEvent[] = await res.json();
            setAuditEvents(data);
            setAuditStatus(`✅ Loaded ${data.length} events`);
        } catch (e) {
            setAuditStatus(`❌ Error loading audit events: ${e}`);
        }
    };

    const exportAuditJson = () => {
        const blob = new Blob([JSON.stringify(auditEvents, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
        a.href = url;
        a.download = `audit_events_${timestamp}.json`;
        a.click();
        URL.revokeObjectURL(url);
    };

    const fetchSettings = async () => {
        try {
            const res = await apiFetch('/api/infrastructure');
            if (res.ok) {
                const data: InfrastructureConfig = await res.json();
                const supervisorList = data.supervisors;
                setSupervisors(supervisorList);
                setOpcServers(data.opc_servers);
                setActiveOpcSecurityServerId('');
                setOpcSecurityConfig(null);
                setOpcTokens([]);
                setHmiClient(data.hmi_client || defaultHmiClientSettings);
                const savedClients = data.controller_host_clients || [];
                const mergedClients = supervisorList.map(sup => (
                    savedClients.find(client => client.supervisor_id === sup.id) || defaultControllerHostClient(sup.id)
                ));
                setControllerHostClients(mergedClients);
            } else {
                const detail = await res.text();
                setInfraStatus(`❌ Failed to load infrastructure settings: ${detail || res.statusText}`);
            }
        } catch (e) {
            setInfraStatus(`❌ Connection error: ${e}`);
        }
    };

    const saveControllerHostClients = async () => {
        setInfraStatus('Saving Controller Host OPC UA client settings...');

        try {
            const res = await apiFetch('/api/infrastructure/controller-host-clients', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    supervisors,
                    controller_host_clients: controllerHostClients,
                }),
            }, true);

            if (res.ok) {
                setInfraStatus('✅ Controller Host OPC UA client settings saved and synced');
            } else {
                setInfraStatus(`❌ Save failed: ${await res.text()}`);
            }
        } catch (e) {
            setInfraStatus(`❌ Error: ${e}`);
        }
    };

    const saveHmiClientSettings = async (reconnectAfterSave: boolean) => {
        setInfraStatus(reconnectAfterSave ? 'Saving and applying HMI OPC UA client settings...' : 'Saving HMI OPC UA client settings...');
        const payload: InfrastructureConfig = {
            supervisors,
            opc_servers: opcServers,
            hmi_client: hmiClient,
            controller_host_clients: controllerHostClients,
        };

        try {
            const saveRes = await apiFetch('/api/infrastructure', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(payload),
            }, true);

            if (!saveRes.ok) {
                setInfraStatus(`❌ Save failed: ${await saveRes.text()}`);
                return;
            }

            if (reconnectAfterSave) {
                const reconnectRes = await apiFetch('/api/infrastructure/opc-reconnect', {
                    method: 'POST',
                }, true);

                if (!reconnectRes.ok) {
                    setInfraStatus(`⚠️ Saved, but reconnect failed: ${await reconnectRes.text()}`);
                    return;
                }

                setInfraStatus('✅ HMI OPC UA client settings saved and reconnect requested');
                return;
            }

            setInfraStatus('✅ HMI OPC UA client settings saved');
        } catch (e) {
            setInfraStatus(`❌ Error: ${e}`);
        }
    };

    const fetchOpcSecurityData = async (opcServerId: string) => {
        if (!opcServerId) return;
        setOpcSecurityStatus('Loading OPC server security settings...');
        try {
            const [cfgRes, tokRes] = await Promise.all([
                apiFetch(`/api/prox/opc/security/config?opc_server_id=${encodeURIComponent(opcServerId)}`),
                apiFetch(`/api/prox/opc/security/tokens?opc_server_id=${encodeURIComponent(opcServerId)}`),
            ]);

            if (!cfgRes.ok) {
                setOpcSecurityStatus(`❌ Failed to load security config: ${await cfgRes.text()}`);
                return;
            }
            if (!tokRes.ok) {
                setOpcSecurityStatus(`❌ Failed to load tokens: ${await tokRes.text()}`);
                return;
            }

            const cfg: OpcSecurityConfig = await cfgRes.json();
            const tokens: OpcUserToken[] = await tokRes.json();
            setOpcSecurityConfig(cfg);
            setOpcTokens(tokens);
            setOpcSecurityStatus('✅ Loaded OPC security settings. Changes require OPC server restart to apply.');
        } catch (e) {
            setOpcSecurityStatus(`❌ Error loading OPC security settings: ${e}`);
        }
    };

    const saveOpcSecurityConfig = async () => {
        if (!activeOpcSecurityServerId || !opcSecurityConfig) return;
        setOpcSecurityStatus('Saving OPC security config...');
        try {
            const sanitizedConfig: OpcSecurityConfig = {
                ...opcSecurityConfig,
                endpoints: opcSecurityConfig.endpoints.map((ep) => {
                    const isNoneNone = ep.security_policy === 'None' && ep.security_mode === 'None';
                    return {
                        ...ep,
                        user_token_ids: isNoneNone
                            ? ep.user_token_ids
                            : ep.user_token_ids.filter((tokenId) => tokenId !== 'ANONYMOUS'),
                    };
                }),
            };

            const res = await apiFetch(
                `/api/prox/opc/security/config?opc_server_id=${encodeURIComponent(activeOpcSecurityServerId)}`,
                {
                    method: 'PUT',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(sanitizedConfig),
                },
                true,
            );
            const text = await res.text();
            if (!res.ok) {
                setOpcSecurityStatus(`❌ Save failed: ${text}`);
                return;
            }
            setOpcSecurityConfig(sanitizedConfig);
            setOpcSecurityStatus(`✅ ${text}`);
        } catch (e) {
            setOpcSecurityStatus(`❌ Save error: ${e}`);
        }
    };

    const addOpcEndpointProfile = () => {
        if (!opcSecurityConfig) return;
        const id = `profile_${Date.now()}`;
        const next: OpcSecurityConfig = {
            ...opcSecurityConfig,
            default_endpoint: opcSecurityConfig.default_endpoint || id,
            endpoints: [
                ...opcSecurityConfig.endpoints,
                {
                    id,
                    path: '/',
                    security_policy: 'Basic256Sha256',
                    security_mode: 'SignAndEncrypt',
                    security_level: 10,
                    user_token_ids: [],
                },
            ],
        };
        setOpcSecurityConfig(next);
    };

    const updateOpcEndpointProfile = (id: string, field: keyof OpcSecurityEndpoint, value: string | number | string[]) => {
        if (!opcSecurityConfig) return;
        setOpcSecurityConfig({
            ...opcSecurityConfig,
            endpoints: opcSecurityConfig.endpoints.map((ep) => (ep.id === id ? { ...ep, [field]: value } : ep)),
        });
    };

    const removeOpcEndpointProfile = (id: string) => {
        if (!opcSecurityConfig) return;
        const endpoints = opcSecurityConfig.endpoints.filter((ep) => ep.id !== id);
        const defaultEndpoint = endpoints.some((ep) => ep.id === opcSecurityConfig.default_endpoint)
            ? opcSecurityConfig.default_endpoint
            : (endpoints[0]?.id || '');
        setOpcSecurityConfig({
            ...opcSecurityConfig,
            default_endpoint: defaultEndpoint,
            endpoints,
        });
    };

    const saveOpcToken = async () => {
        if (!activeOpcSecurityServerId) return;
        if (!opcTokenDraft.user.trim()) {
            setOpcSecurityStatus('❌ Username is required');
            return;
        }
        if (!opcTokenDraft.pass.trim() && !opcTokenDraft.x509.trim()) {
            setOpcSecurityStatus('❌ Provide password or x509 path');
            return;
        }

        setOpcSecurityStatus('Saving token...');
        try {
            const payload: Record<string, string> = {
                user: opcTokenDraft.user.trim(),
            };
            if (opcTokenDraft.pass.trim()) payload.pass = opcTokenDraft.pass;
            if (opcTokenDraft.x509.trim()) payload.x509 = opcTokenDraft.x509.trim();

            const res = await apiFetch(
                `/api/prox/opc/security/tokens?opc_server_id=${encodeURIComponent(activeOpcSecurityServerId)}`,
                {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(payload),
                },
                true,
            );
            const text = await res.text();
            if (!res.ok) {
                setOpcSecurityStatus(`❌ Token save failed: ${text}`);
                return;
            }
            setOpcTokenDraft({ user: '', pass: '', x509: '' });
            await fetchOpcSecurityData(activeOpcSecurityServerId);
            setOpcSecurityStatus(`✅ ${text}`);
        } catch (e) {
            setOpcSecurityStatus(`❌ Token save error: ${e}`);
        }
    };

    const removeOpcToken = async (tokenId: string) => {
        if (!activeOpcSecurityServerId) return;
        setOpcSecurityStatus(`Removing token '${tokenId}'...`);
        try {
            const res = await apiFetch(
                `/api/prox/opc/security/tokens/${encodeURIComponent(tokenId)}?opc_server_id=${encodeURIComponent(activeOpcSecurityServerId)}`,
                { method: 'DELETE' },
                true,
            );
            const text = await res.text();
            if (!res.ok) {
                setOpcSecurityStatus(`❌ Token remove failed: ${text}`);
                return;
            }
            await fetchOpcSecurityData(activeOpcSecurityServerId);
            setOpcSecurityStatus(`✅ ${text}`);
        } catch (e) {
            setOpcSecurityStatus(`❌ Token remove error: ${e}`);
        }
    };

    const restartOpcServer = async () => {
        if (!activeOpcSecurityServerId) return;
        setOpcSecurityStatus('Requesting OPC server restart...');
        try {
            const res = await apiFetch(
                `/api/prox/opc/restart?opc_server_id=${encodeURIComponent(activeOpcSecurityServerId)}`,
                { method: 'POST' },
                true,
            );
            const text = await res.text();
            if (!res.ok) {
                setOpcSecurityStatus(`❌ Restart failed: ${text}`);
                return;
            }
            setOpcSecurityStatus(`✅ ${text}`);
        } catch (e) {
            setOpcSecurityStatus(`❌ Restart error: ${e}`);
        }
    };

    const fetchUsers = async () => {
        try {
            const res = await apiFetch('/api/admin/users');
            if (!res.ok) {
                setUsersStatus(`❌ Failed to load users: ${await res.text()}`);
                return;
            }
            const data: ManagedUser[] = await res.json();
            setUsers(data);
        } catch (e) {
            setUsersStatus(`❌ Error loading users: ${e}`);
        }
    };

    const createUser = async () => {
        if (!newUsername.trim() || !newPassword) {
            setUsersStatus('❌ Username and password are required');
            return;
        }

        setUsersStatus('Creating user...');
        const res = await apiFetch('/api/admin/users', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                username: newUsername.trim(),
                password: newPassword,
                role: newRole,
            }),
        }, true);

        if (!res.ok) {
            setUsersStatus(`❌ Create failed: ${await res.text()}`);
            return;
        }

        setNewUsername('');
        setNewPassword('');
        setNewRole('viewer');
        setUsersStatus('✅ User created');
        await fetchUsers();
    };

    const updateRole = async (userId: number, role: UserRole) => {
        const res = await apiFetch(`/api/admin/users/${userId}/role`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ role }),
        }, true);

        if (!res.ok) {
            setUsersStatus(`❌ Role update failed: ${await res.text()}`);
            return;
        }

        setUsersStatus('✅ Role updated');
        await fetchUsers();
    };

    const setDisabled = async (userId: number, disabled: boolean) => {
        const res = await apiFetch(`/api/admin/users/${userId}/disable`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ disabled }),
        }, true);

        if (!res.ok) {
            setUsersStatus(`❌ Update failed: ${await res.text()}`);
            return;
        }

        setUsersStatus(disabled ? '✅ User disabled' : '✅ User enabled');
        await fetchUsers();
    };

    const resetPassword = async (userId: number) => {
        const newPass = resetPasswords[userId];
        if (!newPass) {
            setUsersStatus('❌ Enter a new password first');
            return;
        }

        const res = await apiFetch(`/api/admin/users/${userId}/password`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ new_password: newPass }),
        }, true);

        if (!res.ok) {
            setUsersStatus(`❌ Password reset failed: ${await res.text()}`);
            return;
        }

        setUsersStatus('✅ Password reset (recovery)');
        setResetPasswords(prev => ({ ...prev, [userId]: '' }));
    };

    const addSupervisor = () => {
        const newSup = { id: crypto.randomUUID(), name: 'New Supervisor', url: 'http://127.0.0.1:8080' };
        setSupervisors([...supervisors, newSup]);
        setControllerHostClients([...controllerHostClients, defaultControllerHostClient(newSup.id)]);
    };

    const addOpc = () => {
        const primary = opcServers[0];
        const newOpc = {
            id: crypto.randomUUID(),
            name: 'New OPC Server',
            url: 'http://127.0.0.1:9090',
            opc_endpoint: 'opc.tcp://localhost:4855',
            security_policy: primary?.security_policy || hmiClient.security_policy,
            security_mode: primary?.security_mode || hmiClient.security_mode,
            auth_mode: primary?.auth_mode || hmiClient.auth_mode,
        };
        setOpcServers([...opcServers, newOpc]);
    };

    const updateSupervisor = (id: string, field: keyof ServiceConfig, value: string) => {
        setSupervisors(supervisors.map(s => (s.id === id ? { ...s, [field]: value } : s)));
    };

    const updateOpc = (id: string, field: keyof ServiceConfig, value: string) => {
        setOpcServers(opcServers.map(s => (s.id === id ? { ...s, [field]: value } : s)));
    };

    const removeSupervisor = (id: string) => {
        setSupervisors(supervisors.filter(s => s.id !== id));
        setControllerHostClients(controllerHostClients.filter(client => client.supervisor_id !== id));
    };

    const removeOpc = (id: string) => {
        const next = opcServers.filter(s => s.id !== id);
        setOpcServers(next);
        if (activeOpcSecurityServerId === id) {
            setActiveOpcSecurityServerId(next[0]?.id || '');
        }
    };

    const updateHmiClient = (field: keyof HmiClientSettings, value: string) => {
        setHmiClient(prev => ({ ...prev, [field]: value }));
    };

    const updateControllerHostClient = (supervisorId: string, field: keyof ControllerHostClientSettings, value: string) => {
        setControllerHostClients(prev => prev.map(client => (
            client.supervisor_id === supervisorId ? { ...client, [field]: value } : client
        )));
    };

    const toggleOpcSecurityPanel = (opcId: string) => {
        if (activeOpcSecurityServerId === opcId) {
            setActiveOpcSecurityServerId('');
            return;
        }
        setActiveOpcSecurityServerId(opcId);
    };

    return (
        <div className="p-6 space-y-6 animate-in fade-in duration-300 h-full overflow-y-auto">
            <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
                <div className="bg-gradient-to-r from-slate-800 to-slate-900 px-6 py-4 border-b-2 border-slate-700 flex justify-between items-center">
                    <div>
                        <h2 className="text-xl font-bold text-white tracking-wide flex items-center gap-2">
                            ⚙️ Settings
                        </h2>
                        <p className="text-xs text-slate-400 mt-1 uppercase tracking-wider font-semibold">
                            Administrative Configuration
                        </p>
                    </div>
                </div>
                <div className="px-4 py-2 flex gap-2 bg-slate-950/70 border-b border-slate-800">
                    {visibleTabs.map(tab => (
                        <button
                            key={tab}
                            onClick={() => setActiveTab(tab)}
                            className={`px-3 py-1 rounded text-xs font-bold ${
                                activeTab === tab ? 'bg-indigo-600 text-white' : 'bg-slate-800 text-slate-400 hover:text-slate-200'
                            }`}
                        >
                            {tab === 'Infrastructure' ? (
                                <span className="inline-flex items-center gap-1"><Server size={12}/>Infrastructure</span>
                            ) : tab === 'Users' ? (
                                <span className="inline-flex items-center gap-1"><Users size={12}/>Users</span>
                            ) : (
                                <span className="inline-flex items-center gap-1"><ClipboardList size={12}/>Audit</span>
                            )}
                        </button>
                    ))}
                </div>
            </div>

            {activeTab === 'Infrastructure' && (
                <>
                    <div className="text-sm font-bold text-center mb-2">
                        {infraStatus && <span className={infraStatus.includes('✅') ? 'text-indigo-300' : 'text-slate-300'}>{infraStatus}</span>}
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-indigo-900/30 overflow-hidden shadow-lg">
                        <div className="bg-indigo-900/20 px-4 py-2 border-b border-indigo-900/30 flex justify-between items-center">
                            <h3 className="font-bold text-indigo-300 text-sm uppercase flex items-center gap-2">
                                <Server size={16}/> Controller Host Registry
                            </h3>
                            <button onClick={addSupervisor} className="text-xs bg-indigo-600 hover:bg-indigo-500 text-white px-2 py-1 rounded flex items-center gap-1">
                                <Plus size={12}/> Add
                            </button>
                        </div>
                        <div className="p-4 space-y-3">
                            {supervisors.map(sup => (
                                <div key={sup.id} className="flex gap-2 items-center bg-slate-800/50 p-2 rounded border border-slate-700">
                                    <input
                                        value={sup.name}
                                        onChange={(e) => updateSupervisor(sup.id, 'name', e.target.value)}
                                        className="bg-transparent border-b border-slate-600 text-slate-200 text-xs w-1/3 focus:outline-none focus:border-indigo-500 px-1 py-1"
                                        placeholder="Controller Host Name"
                                    />
                                    <input
                                        value={sup.url}
                                        onChange={(e) => updateSupervisor(sup.id, 'url', e.target.value)}
                                        className="bg-transparent border-b border-slate-600 text-indigo-300 font-mono text-xs flex-1 focus:outline-none focus:border-indigo-500 px-1 py-1"
                                        placeholder="Controller Host URL (http://...)"
                                    />
                                    <button onClick={() => removeSupervisor(sup.id)} className="text-slate-500 hover:text-indigo-300">
                                        <Trash2 size={14}/>
                                    </button>
                                </div>
                            ))}
                        </div>
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-indigo-900/30 overflow-hidden shadow-lg">
                        <div className="bg-indigo-900/20 px-4 py-2 border-b border-indigo-900/30 flex justify-between items-center">
                            <h3 className="font-bold text-indigo-300 text-sm uppercase flex items-center gap-2">
                                <Network size={16}/> OPC UA Servers
                            </h3>
                            <button onClick={addOpc} className="text-xs bg-indigo-600 hover:bg-indigo-500 text-white px-2 py-1 rounded flex items-center gap-1">
                                <Plus size={12}/> Add
                            </button>
                        </div>
                        <div className="p-4 space-y-3">
                            {opcServers.map(opc => (
                                <div key={opc.id} className="bg-slate-800/50 p-3 rounded border border-slate-700 space-y-2">
                                    <div className="flex gap-2 items-center">
                                        <input
                                            value={opc.name}
                                            onChange={(e) => updateOpc(opc.id, 'name', e.target.value)}
                                            className="bg-transparent border-b border-slate-600 text-slate-200 text-xs flex-1 focus:outline-none focus:border-indigo-500 px-1 py-1"
                                        />
                                        <button onClick={() => removeOpc(opc.id)} className="text-slate-500 hover:text-indigo-300">
                                            <Trash2 size={14}/>
                                        </button>
                                    </div>
                                    <input
                                        value={opc.url}
                                        onChange={(e) => updateOpc(opc.id, 'url', e.target.value)}
                                        className="w-full bg-transparent border-b border-slate-600 text-indigo-300 font-mono text-xs focus:outline-none focus:border-indigo-500 px-1 py-1"
                                        placeholder="http://host:port"
                                    />
                                    <input
                                        value={opc.opc_endpoint || ''}
                                        onChange={(e) => updateOpc(opc.id, 'opc_endpoint', e.target.value)}
                                        className="w-full bg-transparent border-b border-slate-600 text-indigo-300 font-mono text-xs focus:outline-none focus:border-indigo-500 px-1 py-1"
                                        placeholder="opc.tcp://host:port"
                                    />
                                    <div className="pt-1">
                                        <button
                                            onClick={() => toggleOpcSecurityPanel(opc.id)}
                                            className={`text-xs px-2 py-1 rounded border ${activeOpcSecurityServerId === opc.id ? 'bg-indigo-600 border-indigo-500 text-white' : 'bg-slate-900 border-slate-700 text-slate-300 hover:text-white'}`}
                                        >
                                            {activeOpcSecurityServerId === opc.id ? 'Hide Security' : 'Manage Security'}
                                        </button>
                                    </div>

                                    {activeOpcSecurityServerId === opc.id && (
                                        <div className="mt-3 space-y-3 border border-slate-700 rounded-lg p-3 bg-slate-900/50">
                                            {opcSecurityStatus && <div className="text-xs text-slate-300">{opcSecurityStatus}</div>}

                                            {opcSecurityConfig && (
                                                <>
                                                    <div className="flex items-center justify-between gap-2">
                                                        <div className="flex items-center gap-2">
                                                            <label className="text-xs text-slate-300">Default Endpoint</label>
                                                            <select
                                                                value={opcSecurityConfig.default_endpoint}
                                                                onChange={(e) => setOpcSecurityConfig({ ...opcSecurityConfig, default_endpoint: e.target.value })}
                                                                className="bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs"
                                                            >
                                                                {opcSecurityConfig.endpoints.map(ep => (
                                                                    <option key={ep.id} value={ep.id}>{ep.id}</option>
                                                                ))}
                                                            </select>
                                                        </div>
                                                        <button onClick={addOpcEndpointProfile} className="text-xs bg-indigo-600 hover:bg-indigo-500 text-white px-2 py-1 rounded">
                                                            Add Profile
                                                        </button>
                                                    </div>

                                                    <div className="space-y-2">
                                                        {opcSecurityConfig.endpoints.map((ep, idx) => (
                                                            <div key={`${opc.id}-endpoint-${idx}`} className="grid grid-cols-12 gap-2 items-center bg-slate-800/50 p-2 rounded border border-slate-700">
                                                                <div className="col-span-2 space-y-1">
                                                                    <label className="text-[11px] text-slate-400">Endpoint ID</label>
                                                                    <input
                                                                        value={ep.id}
                                                                        onChange={(e) => updateOpcEndpointProfile(ep.id, 'id', e.target.value)}
                                                                        className="w-full bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs"
                                                                    />
                                                                </div>
                                                                <div className="col-span-2 space-y-1">
                                                                    <label className="text-[11px] text-slate-400">Path</label>
                                                                    <input
                                                                        value={ep.path}
                                                                        onChange={(e) => updateOpcEndpointProfile(ep.id, 'path', e.target.value)}
                                                                        className="w-full bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs"
                                                                        placeholder="/"
                                                                    />
                                                                </div>
                                                                <div className="col-span-2 space-y-1">
                                                                    <label className="text-[11px] text-slate-400">Security Policy</label>
                                                                    <select
                                                                        value={ep.security_policy}
                                                                        onChange={(e) => updateOpcEndpointProfile(ep.id, 'security_policy', e.target.value)}
                                                                        className="w-full bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs"
                                                                    >
                                                                        {securityPolicyOptions.map(option => (
                                                                            <option key={option} value={option}>{option}</option>
                                                                        ))}
                                                                    </select>
                                                                </div>
                                                                <div className="col-span-2 space-y-1">
                                                                    <label className="text-[11px] text-slate-400">Security Mode</label>
                                                                    <select
                                                                        value={ep.security_mode}
                                                                        onChange={(e) => updateOpcEndpointProfile(ep.id, 'security_mode', e.target.value)}
                                                                        className="w-full bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs"
                                                                    >
                                                                        {securityModeOptions.map(option => (
                                                                            <option key={option} value={option}>{option}</option>
                                                                        ))}
                                                                    </select>
                                                                </div>
                                                                <div className="col-span-2 space-y-1">
                                                                    <label className="text-[11px] text-slate-400">Security Level</label>
                                                                    <input
                                                                        type="number"
                                                                        value={ep.security_level}
                                                                        onChange={(e) => updateOpcEndpointProfile(ep.id, 'security_level', Number(e.target.value) || 0)}
                                                                        className="w-full bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs"
                                                                        min={0}
                                                                    />
                                                                </div>
                                                                <button onClick={() => removeOpcEndpointProfile(ep.id)} className="col-span-2 text-xs bg-slate-700 hover:bg-slate-600 text-white px-2 py-1 rounded">
                                                                    Remove
                                                                </button>
                                                                <div className="col-span-12 text-[11px] text-slate-400 flex flex-wrap gap-2">
                                                                    {opcTokens
                                                                        .filter((token) => {
                                                                            if (token.id !== 'ANONYMOUS') return true;
                                                                            return ep.security_policy === 'None' && ep.security_mode === 'None';
                                                                        })
                                                                        .map(token => {
                                                                        const checked = ep.user_token_ids.includes(token.id);
                                                                        return (
                                                                            <label key={`${ep.id}-${token.id}`} className="inline-flex items-center gap-1">
                                                                                <input
                                                                                    type="checkbox"
                                                                                    checked={checked}
                                                                                    onChange={(e) => {
                                                                                        const nextIds = e.target.checked
                                                                                            ? [...ep.user_token_ids, token.id]
                                                                                            : ep.user_token_ids.filter(id => id !== token.id);
                                                                                        updateOpcEndpointProfile(ep.id, 'user_token_ids', nextIds);
                                                                                    }}
                                                                                />
                                                                                {token.id}
                                                                            </label>
                                                                        );
                                                                    })}
                                                                </div>
                                                            </div>
                                                        ))}
                                                    </div>

                                                    <div className="pt-1">
                                                        <button onClick={saveOpcSecurityConfig} className="text-xs bg-indigo-600 hover:bg-indigo-500 text-white px-3 py-1.5 rounded">
                                                            Save Security Profiles (Restart Required)
                                                        </button>
                                                        <button
                                                            onClick={() => setShowRestartConfirm(true)}
                                                            className="text-xs bg-slate-700 hover:bg-slate-600 text-white px-3 py-1.5 rounded ml-2"
                                                        >
                                                            Restart OPC Server
                                                        </button>
                                                    </div>

                                                    <div className="pt-2 border-t border-slate-700">
                                                        <div className="text-xs font-bold text-indigo-300 uppercase mb-2">OPC Server User Tokens</div>
                                                        <div className="grid grid-cols-3 gap-2">
                                                            <input
                                                                value={opcTokenDraft.user}
                                                                onChange={(e) => setOpcTokenDraft(prev => ({ ...prev, user: e.target.value }))}
                                                                placeholder="username"
                                                                className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                                            />
                                                            <input
                                                                type="password"
                                                                value={opcTokenDraft.pass}
                                                                onChange={(e) => setOpcTokenDraft(prev => ({ ...prev, pass: e.target.value }))}
                                                                placeholder="password (optional if x509)"
                                                                className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                                            />
                                                            <input
                                                                value={opcTokenDraft.x509}
                                                                onChange={(e) => setOpcTokenDraft(prev => ({ ...prev, x509: e.target.value }))}
                                                                placeholder="x509 path (optional)"
                                                                className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                                            />
                                                        </div>
                                                        <div className="mt-2">
                                                            <button onClick={saveOpcToken} className="text-xs bg-indigo-600 hover:bg-indigo-500 text-white px-3 py-1.5 rounded">
                                                                Save Token (Restart Required)
                                                            </button>
                                                        </div>
                                                        <div className="space-y-2 mt-2">
                                                            {opcTokens
                                                                .filter((token) => {
                                                                    if (token.id !== 'ANONYMOUS') return true;
                                                                    return opcSecurityConfig.endpoints.some((ep) => ep.security_policy === 'None' && ep.security_mode === 'None');
                                                                })
                                                                .map(token => (
                                                                <div key={token.id} className="grid grid-cols-12 gap-2 items-center bg-slate-800/40 border border-slate-700 rounded p-2 text-xs">
                                                                    <div className="col-span-3 text-slate-100 font-bold">{token.id}</div>
                                                                    <div className="col-span-3 text-slate-300">{token.user}</div>
                                                                    <div className="col-span-4 text-slate-400">{token.has_password ? 'password' : ''}{token.has_password && token.x509 ? ' + ' : ''}{token.x509 ? `x509: ${token.x509}` : ''}</div>
                                                                    <div className="col-span-2 text-right">
                                                                        {token.id === 'ANONYMOUS' ? (
                                                                            <span className="text-[11px] text-slate-500">Built-in</span>
                                                                        ) : (
                                                                            <button onClick={() => removeOpcToken(token.id)} className="text-xs bg-slate-700 hover:bg-slate-600 text-white px-2 py-1 rounded">
                                                                                Remove
                                                                            </button>
                                                                        )}
                                                                    </div>
                                                                </div>
                                                            ))}
                                                        </div>
                                                    </div>
                                                </>
                                            )}
                                        </div>
                                    )}
                                </div>
                            ))}
                        </div>
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-indigo-900/30 overflow-hidden shadow-lg">
                        <div className="bg-indigo-900/20 px-4 py-2 border-b border-indigo-900/30">
                            <h3 className="font-bold text-indigo-300 text-sm uppercase">HMI OPC UA Client</h3>
                        </div>
                        <div className="p-4 space-y-3">
                            <div className="text-xs rounded px-3 py-2 border border-slate-700 bg-slate-950/50">
                                <span className="text-slate-300">Connection: </span>
                                <span className={Number(opcConnection) === 1 ? 'text-indigo-300' : 'text-slate-400'}>
                                    {Number(opcConnection) === 1 ? 'Connected' : 'Disconnected'}
                                </span>
                            </div>

                            {typeof opcLastError === 'string' && opcLastError.trim().length > 0 && (
                                <div className="text-xs text-slate-300 bg-slate-900/70 border border-slate-700 rounded px-3 py-2">
                                    ⚠️ Last OPC error: {opcLastError}
                                </div>
                            )}

                            {Number(opcAuthError) === 1 && (
                                <div className="text-xs text-slate-300 bg-slate-900/70 border border-indigo-800 rounded px-3 py-2">
                                    ❌ OPC authentication failed. Check HMI OPC UA username/password or auth mode.
                                    {typeof opcLastError === 'string' && opcLastError.trim().length > 0 && (
                                        <div className="text-slate-400 mt-1">{opcLastError}</div>
                                    )}
                                </div>
                            )}

                            <div className="flex flex-wrap gap-3">
                                <div className="space-y-1">
                                    <label className="text-xs text-slate-300">Security Policy </label>
                                    <select
                                        value={hmiClient.security_policy}
                                        onChange={(e) => updateHmiClient('security_policy', e.target.value)}
                                        className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                    >
                                        {securityPolicyOptions.map(option => (
                                            <option key={option} value={option}>{option}</option>
                                        ))}
                                    </select>
                                </div>
                                <div className="space-y-1">
                                    <label className="text-xs text-slate-300">Security Mode </label>
                                    <select
                                        value={hmiClient.security_mode}
                                        onChange={(e) => updateHmiClient('security_mode', e.target.value)}
                                        className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                    >
                                        {securityModeOptions.map(option => (
                                            <option key={option} value={option}>{option}</option>
                                        ))}
                                    </select>
                                </div>
                                <div className="space-y-1">
                                    <label className="text-xs text-slate-300">Auth Mode </label>
                                    <select
                                        value={hmiClient.auth_mode}
                                        onChange={(e) => updateHmiClient('auth_mode', e.target.value)}
                                        className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                    >
                                        <option value="Anonymous">Anonymous</option>
                                        <option value="Username">Username/Password</option>
                                        <option value="X509">X509 Certificate</option>
                                    </select>
                                </div>
                            </div>

                            {hmiClient.auth_mode === 'Username' && (
                                <div className="flex flex-wrap gap-3">
                                    <div className="space-y-1">
                                        <label className="text-xs text-slate-300">Username </label>
                                        <input
                                            value={hmiClient.username || ''}
                                            onChange={(e) => updateHmiClient('username', e.target.value)}
                                            className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                            placeholder="OPC Username"
                                        />
                                    </div>
                                    <div className="space-y-1">
                                        <label className="text-xs text-slate-300">Password </label>
                                        <input
                                            type="password"
                                            value={hmiClient.password || ''}
                                            onChange={(e) => updateHmiClient('password', e.target.value)}
                                            className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                            placeholder="OPC Password"
                                        />
                                    </div>
                                </div>
                            )}

                            <div className="flex gap-2 pt-2">
                                <button
                                    onClick={() => saveHmiClientSettings(true)}
                                    className="text-xs bg-indigo-600 hover:bg-indigo-500 text-white px-3 py-1.5 rounded"
                                >
                                    Save & Reconnect
                                </button>
                            </div>
                        </div>
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-indigo-900/30 overflow-hidden shadow-lg">
                        <div className="bg-indigo-900/20 px-4 py-2 border-b border-indigo-900/30">
                            <h3 className="font-bold text-indigo-300 text-sm uppercase">Controller Host OPC UA Clients</h3>
                        </div>
                        <div className="p-4 space-y-4">
                            {supervisors.map((supervisor, index) => {
                                const hostClient = controllerHostClients.find(client => client.supervisor_id === supervisor.id)
                                    || defaultControllerHostClient(supervisor.id);
                                return (
                                <div key={supervisor.id} className="space-y-2">
                                    <div className="text-xs font-bold text-slate-200 uppercase">{supervisor.name || 'Controller Host'} OPC UA Client</div>
                                    <div className="flex flex-wrap gap-3">
                                        <div className="space-y-1">
                                            <label className="text-xs text-slate-300">Security Policy </label>
                                            <select
                                                value={hostClient.security_policy}
                                                onChange={(e) => updateControllerHostClient(supervisor.id, 'security_policy', e.target.value)}
                                                className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                            >
                                                {securityPolicyOptions.map(option => (
                                                    <option key={option} value={option}>{option}</option>
                                                ))}
                                            </select>
                                        </div>
                                        <div className="space-y-1">
                                            <label className="text-xs text-slate-300">Security Mode </label>
                                            <select
                                                value={hostClient.security_mode}
                                                onChange={(e) => updateControllerHostClient(supervisor.id, 'security_mode', e.target.value)}
                                                className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                            >
                                                {securityModeOptions.map(option => (
                                                    <option key={option} value={option}>{option}</option>
                                                ))}
                                            </select>
                                        </div>
                                        <div className="space-y-1">
                                            <label className="text-xs text-slate-300">Auth Mode </label>
                                            <select
                                                value={hostClient.auth_mode}
                                                onChange={(e) => updateControllerHostClient(supervisor.id, 'auth_mode', e.target.value)}
                                                className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                            >
                                                <option value="Anonymous">Anonymous</option>
                                                <option value="Username">Username/Password</option>
                                                <option value="X509">X509 Certificate</option>
                                            </select>
                                        </div>
                                    </div>

                                    {hostClient.auth_mode === 'Username' && (
                                        <div className="flex flex-wrap gap-3">
                                            <div className="space-y-1">
                                                <label className="text-xs text-slate-300">Username </label>
                                                <input
                                                    value={hostClient.username || ''}
                                                    onChange={(e) => updateControllerHostClient(supervisor.id, 'username', e.target.value)}
                                                    className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                                    placeholder="OPC Username"
                                                />
                                            </div>
                                            <div className="space-y-1">
                                                <label className="text-xs text-slate-300">Password </label>
                                                <input
                                                    type="password"
                                                    value={hostClient.password || ''}
                                                    onChange={(e) => updateControllerHostClient(supervisor.id, 'password', e.target.value)}
                                                    className="w-56 bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                                    placeholder="OPC Password"
                                                />
                                            </div>
                                        </div>
                                    )}

                                    {index < supervisors.length - 1 && (
                                        <div className="pt-2 border-b border-slate-700/60" />
                                    )}
                                </div>
                            )})}

                            <div className="flex gap-2 pt-2">
                                <button
                                    onClick={saveControllerHostClients}
                                    className="text-xs bg-indigo-600 hover:bg-indigo-500 text-white px-3 py-1.5 rounded"
                                >
                                    Save
                                </button>
                            </div>
                        </div>
                    </div>

                </>
            )}

            {activeTab === 'Users' && (
                <>
                    <div className="text-sm font-bold text-center mb-2">
                        {usersStatus && <span className={usersStatus.includes('✅') ? 'text-indigo-300' : 'text-slate-300'}>{usersStatus}</span>}
                    </div>

                    <div className="text-xs text-slate-400 text-center -mt-1 mb-2">
                        Users can change their own password from the account panel. This section is for admin reset/recovery.
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-indigo-900/30 overflow-hidden shadow-lg">
                        <div className="bg-indigo-900/20 px-4 py-2 border-b border-indigo-900/30 flex items-center gap-2">
                            <UserPlus size={16} className="text-indigo-300"/>
                            <h3 className="font-bold text-indigo-300 text-sm uppercase">Add User</h3>
                        </div>
                        <div className="p-4 grid grid-cols-4 gap-2">
                            <input
                                value={newUsername}
                                onChange={(e) => setNewUsername(e.target.value)}
                                placeholder="Username"
                                className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-sm"
                            />
                            <input
                                value={newPassword}
                                onChange={(e) => setNewPassword(e.target.value)}
                                placeholder="Temp password"
                                type="password"
                                className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-sm"
                            />
                            <select
                                value={newRole}
                                onChange={(e) => setNewRole(e.target.value as UserRole)}
                                className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-sm"
                            >
                                {roleOptions.map(r => (
                                    <option key={r} value={r}>{r}</option>
                                ))}
                            </select>
                            <button
                                onClick={createUser}
                                className="bg-indigo-600 hover:bg-indigo-500 rounded text-white font-bold text-sm"
                            >
                                Create
                            </button>
                        </div>
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-slate-700/50 overflow-hidden shadow-lg">
                        <div className="bg-slate-800/80 px-4 py-2 border-b border-slate-700 flex justify-between items-center">
                            <h3 className="font-bold text-slate-100 text-sm uppercase inline-flex items-center gap-2"><UserCog size={14}/> Users</h3>
                            <button onClick={fetchUsers} className="text-xs bg-slate-700 hover:bg-slate-600 px-2 py-1 rounded">Refresh</button>
                        </div>
                        <div className="p-4 space-y-3">
                            {users.map(u => (
                                <div key={u.id} className="grid grid-cols-12 gap-2 items-center bg-slate-800/40 border border-slate-700 rounded p-2">
                                    <div className="col-span-2 text-sm font-bold text-slate-100">{u.username}</div>
                                    <div className="col-span-2 text-xs text-slate-500">#{u.id}</div>
                                    <div className="col-span-2">
                                        <select
                                            value={u.role}
                                            onChange={(e) => updateRole(u.id, e.target.value as UserRole)}
                                            className="w-full bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs"
                                        >
                                            {roleOptions.map(r => (
                                                <option key={r} value={r}>{r}</option>
                                            ))}
                                        </select>
                                    </div>
                                    <div className="col-span-2">
                                        <button
                                            onClick={() => setDisabled(u.id, !u.disabled)}
                                            className={`w-full text-xs rounded px-2 py-1 ${u.disabled ? 'bg-indigo-700 hover:bg-indigo-600' : 'bg-slate-700 hover:bg-slate-600'} text-white`}
                                        >
                                            {u.disabled ? 'Enable' : 'Disable'}
                                        </button>
                                    </div>
                                    <div className="col-span-3">
                                        <input
                                            type="password"
                                            value={resetPasswords[u.id] || ''}
                                            onChange={(e) => setResetPasswords(prev => ({ ...prev, [u.id]: e.target.value }))}
                                            placeholder="Reset password"
                                            className="w-full bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs"
                                        />
                                    </div>
                                    <div className="col-span-1">
                                        <button
                                            onClick={() => resetPassword(u.id)}
                                            className="w-full bg-indigo-700 hover:bg-indigo-600 text-white rounded px-2 py-1"
                                            title="Admin reset password"
                                        >
                                            <KeyRound size={12}/>
                                        </button>
                                    </div>
                                </div>
                            ))}
                        </div>
                    </div>
                </>
            )}

            {activeTab === 'Audit' && (
                <>
                    <div className="text-sm font-bold text-center mb-2">
                        {auditStatus && <span className={auditStatus.includes('✅') ? 'text-indigo-300' : 'text-slate-300'}>{auditStatus}</span>}
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-indigo-900/30 overflow-hidden shadow-lg">
                        <div className="bg-indigo-900/20 px-4 py-2 border-b border-indigo-900/30 flex justify-between items-center">
                            <h3 className="font-bold text-indigo-300 text-sm uppercase inline-flex items-center gap-2">
                                <ClipboardList size={14}/> Audit Events
                            </h3>
                            <div className="flex items-center gap-2">
                                <button onClick={fetchAudit} className="text-xs bg-slate-700 hover:bg-slate-600 px-2 py-1 rounded">Refresh</button>
                                <button onClick={exportAuditJson} className="text-xs bg-indigo-700 hover:bg-indigo-600 px-2 py-1 rounded text-white inline-flex items-center gap-1">
                                    <Download size={12}/> Export
                                </button>
                            </div>
                        </div>
                        <div className="p-4 space-y-3">
                            <div className="grid grid-cols-5 gap-2">
                                <input
                                    value={auditActor}
                                    onChange={(e) => setAuditActor(e.target.value)}
                                    placeholder="Actor contains..."
                                    className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                />
                                <input
                                    value={auditAction}
                                    onChange={(e) => setAuditAction(e.target.value)}
                                    placeholder="Action contains..."
                                    className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                />
                                <select
                                    value={auditResult}
                                    onChange={(e) => setAuditResult(e.target.value)}
                                    className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                >
                                    <option value="">All results</option>
                                    <option value="success">success</option>
                                    <option value="denied">denied</option>
                                </select>
                                <input
                                    type="number"
                                    value={auditLimit}
                                    onChange={(e) => setAuditLimit(Number(e.target.value) || 200)}
                                    className="bg-slate-950 border border-slate-700 rounded px-2 py-2 text-xs"
                                    min={1}
                                    max={1000}
                                />
                                <button onClick={fetchAudit} className="bg-indigo-700 hover:bg-indigo-600 rounded text-white font-bold text-xs">
                                    Apply
                                </button>
                            </div>

                            <div className="space-y-2 max-h-[480px] overflow-y-auto">
                                {auditEvents.map(event => (
                                    <div key={event.id} className="bg-slate-800/40 border border-slate-700 rounded p-2 text-xs">
                                        <div className="flex items-center justify-between">
                                            <div className="font-bold text-slate-200">#{event.id} · {event.action}</div>
                                            <div className="text-slate-500">{event.created_at}</div>
                                        </div>
                                        <div className="mt-1 text-slate-400">
                                            actor: <span className="text-slate-200">{event.actor_username || '-'}</span> ({event.actor_role || '-'}) · result: <span className={event.result === 'success' ? 'text-emerald-400' : 'text-rose-400'}>{event.result}</span>
                                        </div>
                                        {event.target && <div className="text-slate-400">target: <span className="text-slate-200">{event.target}</span></div>}
                                        {event.detail && <div className="text-slate-500">detail: {event.detail}</div>}
                                    </div>
                                ))}
                            </div>
                        </div>
                    </div>
                </>
            )}

            {showRestartConfirm && (
                <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/70 px-4">
                    <div className="w-full max-w-md rounded-xl border border-slate-700 bg-slate-900 shadow-2xl">
                        <div className="px-4 py-3 border-b border-slate-700">
                            <h4 className="text-sm font-bold text-slate-100">Confirm OPC Server Restart</h4>
                        </div>
                        <div className="px-4 py-4 text-sm text-slate-300">
                            are you sure you want to restart the server
                        </div>
                        <div className="px-4 py-3 border-t border-slate-700 flex justify-end gap-2">
                            <button
                                onClick={() => setShowRestartConfirm(false)}
                                className="text-xs bg-slate-700 hover:bg-slate-600 text-white px-3 py-1.5 rounded"
                            >
                                Cancel
                            </button>
                            <button
                                onClick={async () => {
                                    setShowRestartConfirm(false);
                                    await restartOpcServer();
                                }}
                                className="text-xs bg-indigo-600 hover:bg-indigo-500 text-white px-3 py-1.5 rounded"
                            >
                                Yes, Restart
                            </button>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
};
