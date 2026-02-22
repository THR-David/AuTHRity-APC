import { Activity, Cpu, Layers, Settings } from "lucide-react";
import { useEffect, useRef, useState } from "react";

interface SidebarProps {
    activeModel: string;
    onSelect: (model: string) => void;
    allowGenerator: boolean;
    allowSettings: boolean;
    username: string;
    role: string;
    onChangePassword: () => void;
    onLogout: () => void;
}


export const Sidebar = ({ activeModel, onSelect, allowGenerator, allowSettings, username, role, onChangePassword, onLogout }: SidebarProps) => {
    const [modelList, setModelList] = useState<string[]>([]);
    const [accountMenuOpen, setAccountMenuOpen] = useState(false);
    const accountMenuRef = useRef<HTMLDivElement | null>(null);

    useEffect(() => {
        // Fetch list of available models from Backend
        fetch('/api/models')
            .then(res => res.json())
            .then(data => setModelList(data))
            .catch(err => console.error("Failed to load model list", err));
    }, []);

    useEffect(() => {
        if (!accountMenuOpen) return;
        const onMouseDown = (event: MouseEvent) => {
            if (!accountMenuRef.current?.contains(event.target as Node)) {
                setAccountMenuOpen(false);
            }
        };
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key === 'Escape') {
                setAccountMenuOpen(false);
            }
        };
        document.addEventListener('mousedown', onMouseDown);
        document.addEventListener('keydown', onKeyDown);
        return () => {
            document.removeEventListener('mousedown', onMouseDown);
            document.removeEventListener('keydown', onKeyDown);
        };
    }, [accountMenuOpen]);

    const handleChangePassword = () => {
        setAccountMenuOpen(false);
        onChangePassword();
    };

    const handleLogout = () => {
        setAccountMenuOpen(false);
        onLogout();
    };

    return (
        <aside className="w-64 bg-slate-900 border-r border-slate-800 flex flex-col h-screen shrink-0">
            {/* Logo Area */}
            <div className="h-16 flex items-center gap-3 px-6 border-b border-slate-800 bg-slate-950">
                <div className="p-2 bg-indigo-600 rounded-lg">
                    <Activity size={20} className="text-white" />
                </div>
                <h1 className="font-bold text-lg tracking-wider text-slate-100">AuTHRity</h1>
            </div>

            {/* Menu */}
            <nav className="flex-1 p-4 space-y-2 overflow-y-auto">
                <button 
                    onClick={() => onSelect("Plant Overview")}
                    className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-bold transition-all duration-200 mb-4
                    ${activeModel === "Plant Overview" 
                        ? "bg-indigo-500 text-white shadow-lg shadow-indigo-500/20" 
                        : "bg-slate-800 text-slate-300 hover:bg-slate-700"
                    }`}
                >
                    <Layers size={18} />
                    Plant Overview
                </button>

                <div className="text-xs font-bold text-slate-500 uppercase px-3 mb-2">Controllers</div>
                
                {/* DYNAMIC MODEL LIST */}
                {modelList.map((modelName) => (
                    <button 
                        key={modelName}
                        onClick={() => onSelect(modelName)}
                        className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200
                        ${activeModel === modelName 
                            ? "bg-indigo-600/10 text-indigo-400 border border-indigo-600/20 shadow-[0_0_15px_rgba(79,70,229,0.1)]" 
                            : "text-slate-400 hover:bg-slate-800/50 hover:text-slate-200"
                        }`}
                    >
                        <Cpu size={18} />
                        {modelName}
                    </button>
                ))}

                <div className="my-4 border-b border-slate-800"></div>

                <div className="text-xs font-bold text-slate-500 uppercase px-3 mb-2">Tools</div>
                
                {allowGenerator && (
                    <button 
                        onClick={() => onSelect("Generator")}
                        className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200
                        ${activeModel === "Generator" 
                            ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20" 
                            : "text-slate-400 hover:bg-slate-800/50 hover:text-slate-200"
                        }`}
                    >
                        <Layers size={18} />
                        Generator
                    </button>
                )}
            </nav>

            {/* Footer */}
            <div className="p-4 border-t border-slate-800 bg-slate-950 space-y-2">
                {allowSettings && (
                    <button 
                        onClick={() => onSelect("Settings")}
                        className={`w-full flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-bold transition-all duration-200
                        ${activeModel === "Settings" 
                            ? "bg-slate-800 text-white" 
                            : "text-slate-500 hover:bg-slate-900 hover:text-slate-300"
                        }`}
                    >
                        <Settings size={18} />
                        Settings
                    </button>
                )}
                
                <div className="flex items-center gap-3 text-slate-400 text-sm px-2 pt-2 border-t border-slate-900">
                    <div className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></div>
                    <span>System Online</span>
                </div>

                <div className="mt-2 rounded-lg border border-slate-800 bg-slate-900/60 p-2 text-xs">
                    <div className="flex items-center justify-between gap-2">
                        <div className="text-slate-500">Logged in as</div>
                        <div className="relative" ref={accountMenuRef}>
                            <button
                                onClick={() => setAccountMenuOpen(open => !open)}
                                className="inline-flex items-center justify-center rounded p-1.5 text-slate-400 hover:text-indigo-300 hover:bg-slate-800"
                                title="Account settings"
                                aria-label="Account settings"
                            >
                                <Settings size={14} />
                            </button>
                            {accountMenuOpen && (
                                <div className="absolute right-0 bottom-full mb-1 w-36 rounded border border-slate-700 bg-slate-900 shadow-lg z-20">
                                    <button
                                        onClick={handleChangePassword}
                                        className="w-full text-left px-3 py-2 text-xs text-slate-200 hover:bg-slate-800"
                                    >
                                        Change password
                                    </button>
                                    <button
                                        onClick={handleLogout}
                                        className="w-full text-left px-3 py-2 text-xs text-slate-200 hover:bg-slate-800"
                                    >
                                        Logout
                                    </button>
                                </div>
                            )}
                        </div>
                    </div>
                    <div className="mt-0.5 flex items-center justify-between gap-2">
                        <div className="text-slate-200 font-bold truncate">{username}</div>
                        <div className="text-indigo-400 uppercase tracking-wider shrink-0">{role}</div>
                    </div>
                </div>
            </div>
        </aside>
    );
};