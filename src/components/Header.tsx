import React from "react";
import { Zap, Cpu } from "lucide-react";

export const Header: React.FC = () => {
  return (
    <header className="flex items-center justify-between px-6 py-3.5 bg-neutral-900/70 border-b border-neutral-800/80 backdrop-blur-md shrink-0">
      <div className="flex items-center gap-3">
        <div className="flex items-center justify-center w-9 h-9 rounded-xl bg-gradient-to-tr from-cyan-500 via-indigo-600 to-purple-600 shadow-lg shadow-indigo-500/20">
          <Zap className="w-5 h-5 text-white fill-white/20" />
        </div>
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-base font-bold tracking-tight bg-gradient-to-r from-neutral-100 via-neutral-200 to-neutral-400 bg-clip-text text-transparent">
              Gen Downloader
            </h1>
            <span className="text-[10px] uppercase font-semibold px-1.5 py-0.5 rounded bg-neutral-800 text-neutral-400 border border-neutral-700/60">
              v2.0
            </span>
          </div>
          <p className="text-xs text-neutral-400">High-Performance Media Engine</p>
        </div>
      </div>

      {/* Dynamic GPU Status Badge */}
      <div className="flex items-center gap-2 px-3 py-1 rounded-full bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 text-xs font-medium shadow-sm shadow-emerald-950">
        <span className="relative flex h-2 w-2">
          <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
          <span className="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
        </span>
        <Cpu className="w-3.5 h-3.5" />
        <span>GPU Acceleration: NVENC Ready</span>
      </div>
    </header>
  );
};
