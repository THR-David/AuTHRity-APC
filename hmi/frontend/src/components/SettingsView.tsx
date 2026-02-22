import React, { useState, useEffect } from 'react';
import { Save, Plus, Trash2, Server, Network, Users, UserPlus, KeyRound, UserCog, ClipboardList, Download } from 'lucide-react';
import { apiFetch, type UserRole } from '../lib/api';

export interface ServiceConfig {
    id: string;
    name: string;
    url: string;
    opc_endpoint?: string;
}

interface InfrastructureConfig {
    supervisors: ServiceConfig[];
    opc_servers: ServiceConfig[];
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

export const SettingsView: React.FC = () => {
    const [activeTab, setActiveTab] = useState<'Infrastructure' | 'Users' | 'Audit'>('Infrastructure');

    const [supervisors, setSupervisors] = useState<ServiceConfig[]>([]);
    const [opcServers, setOpcServers] = useState<ServiceConfig[]>([]);
    const [infraStatus, setInfraStatus] = useState<string>('');

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

    useEffect(() => {
        fetchSettings();
        fetchUsers();
        fetchAudit();
    }, []);

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
                setSupervisors(data.supervisors);
                setOpcServers(data.opc_servers);
            } else {
                setInfraStatus('❌ Failed to load infrastructure settings');
            }
        } catch (e) {
            setInfraStatus(`❌ Connection error: ${e}`);
        }
    };

    const saveSettings = async () => {
        setInfraStatus('Saving...');
        const payload: InfrastructureConfig = {
            supervisors,
            opc_servers: opcServers,
        };

        try {
            const res = await apiFetch('/api/infrastructure', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(payload),
            }, true);

            if (res.ok) {
                setInfraStatus('✅ Infrastructure settings saved');
            } else {
                setInfraStatus(`❌ Save failed: ${await res.text()}`);
            }
        } catch (e) {
            setInfraStatus(`❌ Error: ${e}`);
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
    };

    const addOpc = () => {
        const newOpc = {
            id: crypto.randomUUID(),
            name: 'New OPC Server',
            url: 'http://127.0.0.1:9090',
            opc_endpoint: 'opc.tcp://127.0.0.1:4840',
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
    };

    const removeOpc = (id: string) => {
        setOpcServers(opcServers.filter(s => s.id !== id));
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
                    {(['Infrastructure', 'Users', 'Audit'] as const).map(tab => (
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
                        {infraStatus && <span className={infraStatus.includes('✅') ? 'text-emerald-400' : 'text-rose-400'}>{infraStatus}</span>}
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-indigo-900/30 overflow-hidden shadow-lg">
                        <div className="bg-indigo-900/20 px-4 py-2 border-b border-indigo-900/30 flex justify-between items-center">
                            <h3 className="font-bold text-indigo-300 text-sm uppercase flex items-center gap-2">
                                <Server size={16}/> Supervisor Registry
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
                                        placeholder="Supervisor Name"
                                    />
                                    <input
                                        value={sup.url}
                                        onChange={(e) => updateSupervisor(sup.id, 'url', e.target.value)}
                                        className="bg-transparent border-b border-slate-600 text-indigo-300 font-mono text-xs flex-1 focus:outline-none focus:border-indigo-500 px-1 py-1"
                                        placeholder="Management URL (http://...)"
                                    />
                                    <button onClick={() => removeSupervisor(sup.id)} className="text-slate-500 hover:text-rose-400">
                                        <Trash2 size={14}/>
                                    </button>
                                </div>
                            ))}
                        </div>
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-cyan-900/30 overflow-hidden shadow-lg">
                        <div className="bg-cyan-900/20 px-4 py-2 border-b border-cyan-900/30 flex justify-between items-center">
                            <h3 className="font-bold text-cyan-300 text-sm uppercase flex items-center gap-2">
                                <Network size={16}/> OPC UA Connectivity Nodes
                            </h3>
                            <button onClick={addOpc} className="text-xs bg-cyan-600 hover:bg-cyan-500 text-white px-2 py-1 rounded flex items-center gap-1">
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
                                            className="bg-transparent border-b border-slate-600 text-slate-200 text-xs flex-1 focus:outline-none focus:border-cyan-500 px-1 py-1"
                                        />
                                        <button onClick={() => removeOpc(opc.id)} className="text-slate-500 hover:text-rose-400">
                                            <Trash2 size={14}/>
                                        </button>
                                    </div>
                                    <input
                                        value={opc.url}
                                        onChange={(e) => updateOpc(opc.id, 'url', e.target.value)}
                                        className="w-full bg-transparent border-b border-slate-600 text-cyan-300 font-mono text-xs focus:outline-none focus:border-cyan-500 px-1 py-1"
                                        placeholder="http://host:port"
                                    />
                                    <input
                                        value={opc.opc_endpoint || ''}
                                        onChange={(e) => updateOpc(opc.id, 'opc_endpoint', e.target.value)}
                                        className="w-full bg-transparent border-b border-slate-600 text-emerald-300 font-mono text-xs focus:outline-none focus:border-emerald-500 px-1 py-1"
                                        placeholder="opc.tcp://host:port"
                                    />
                                </div>
                            ))}
                        </div>
                    </div>

                    <button
                        onClick={saveSettings}
                        className="w-full py-3 bg-gradient-to-r from-emerald-600 to-emerald-500 hover:from-emerald-500 hover:to-emerald-400 text-white font-bold rounded shadow-lg shadow-emerald-900/30 transition-all flex justify-center items-center gap-2"
                    >
                        <Save size={18} /> Save Infrastructure
                    </button>
                </>
            )}

            {activeTab === 'Users' && (
                <>
                    <div className="text-sm font-bold text-center mb-2">
                        {usersStatus && <span className={usersStatus.includes('✅') ? 'text-emerald-400' : 'text-rose-400'}>{usersStatus}</span>}
                    </div>

                    <div className="text-xs text-slate-400 text-center -mt-1 mb-2">
                        Users can change their own password from the account panel. This section is for admin reset/recovery.
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-violet-900/30 overflow-hidden shadow-lg">
                        <div className="bg-violet-900/20 px-4 py-2 border-b border-violet-900/30 flex items-center gap-2">
                            <UserPlus size={16} className="text-violet-300"/>
                            <h3 className="font-bold text-violet-300 text-sm uppercase">Add User</h3>
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
                                className="bg-violet-600 hover:bg-violet-500 rounded text-white font-bold text-sm"
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
                                            className={`w-full text-xs rounded px-2 py-1 ${u.disabled ? 'bg-emerald-700 hover:bg-emerald-600' : 'bg-rose-700 hover:bg-rose-600'} text-white`}
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
                                            className="w-full bg-amber-700 hover:bg-amber-600 text-white rounded px-2 py-1"
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
                        {auditStatus && <span className={auditStatus.includes('✅') ? 'text-emerald-400' : 'text-rose-400'}>{auditStatus}</span>}
                    </div>

                    <div className="bg-slate-900/50 rounded-xl border-2 border-amber-900/30 overflow-hidden shadow-lg">
                        <div className="bg-amber-900/20 px-4 py-2 border-b border-amber-900/30 flex justify-between items-center">
                            <h3 className="font-bold text-amber-300 text-sm uppercase inline-flex items-center gap-2">
                                <ClipboardList size={14}/> Audit Events
                            </h3>
                            <div className="flex items-center gap-2">
                                <button onClick={fetchAudit} className="text-xs bg-slate-700 hover:bg-slate-600 px-2 py-1 rounded">Refresh</button>
                                <button onClick={exportAuditJson} className="text-xs bg-amber-700 hover:bg-amber-600 px-2 py-1 rounded text-white inline-flex items-center gap-1">
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
                                <button onClick={fetchAudit} className="bg-amber-700 hover:bg-amber-600 rounded text-white font-bold text-xs">
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
        </div>
    );
};
