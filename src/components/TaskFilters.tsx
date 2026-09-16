import React from "react";
import { Trash2 } from "lucide-react";
import { TaskFilter } from "../types/download";

export interface TaskFiltersProps {
  activeFilter: TaskFilter;
  onFilterChange: (filter: TaskFilter) => void;
  counts: {
    all: number;
    active: number;
    completed: number;
    failed: number;
  };
  onClearCompleted: () => void;
}

interface FilterTabConfig {
  id: TaskFilter;
  label: string;
  count: number;
  activeStyle: string;
  badgeActiveStyle: string;
  hoverColor: string;
}

export const TaskFilters: React.FC<TaskFiltersProps> = ({
  activeFilter,
  onFilterChange,
  counts,
  onClearCompleted,
}) => {
  const tabs: FilterTabConfig[] = [
    {
      id: "all",
      label: "All Tasks",
      count: counts.all,
      activeStyle: "bg-neutral-800 text-neutral-100 border-neutral-700/80 shadow-sm",
      badgeActiveStyle: "bg-neutral-700/80 text-neutral-200",
      hoverColor: "hover:text-neutral-200",
    },
    {
      id: "active",
      label: "Active",
      count: counts.active,
      activeStyle: "bg-cyan-500/15 text-cyan-300 border-cyan-500/30 shadow-sm shadow-cyan-950/30",
      badgeActiveStyle: "bg-cyan-500/25 text-cyan-200",
      hoverColor: "hover:text-cyan-300",
    },
    {
      id: "completed",
      label: "Completed",
      count: counts.completed,
      activeStyle: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30 shadow-sm shadow-emerald-950/30",
      badgeActiveStyle: "bg-emerald-500/25 text-emerald-200",
      hoverColor: "hover:text-emerald-300",
    },
    {
      id: "failed",
      label: "Failed",
      count: counts.failed,
      activeStyle: "bg-rose-500/15 text-rose-300 border-rose-500/30 shadow-sm shadow-rose-950/30",
      badgeActiveStyle: "bg-rose-500/25 text-rose-200",
      hoverColor: "hover:text-rose-300",
    },
  ];

  return (
    <div className="flex items-center justify-between px-5 py-3 border-b border-neutral-800/80 shrink-0 bg-neutral-900/60 backdrop-blur-sm gap-4">
      {/* Segmented Filter Tab Group */}
      <div className="flex items-center gap-1.5 p-1 bg-neutral-950/60 border border-neutral-800/80 rounded-xl">
        {tabs.map((tab) => {
          const isActive = activeFilter === tab.id;

          return (
            <button
              key={tab.id}
              type="button"
              onClick={() => onFilterChange(tab.id)}
              className={`group px-3 py-1.5 rounded-lg text-xs font-medium border flex items-center transition-all cursor-pointer ${
                isActive
                  ? tab.activeStyle
                  : `text-neutral-400 border-transparent hover:bg-neutral-800/50 ${tab.hoverColor}`
              }`}
            >
              <span>{tab.label}</span>
              <span
                className={`ml-2 px-1.5 py-0.5 rounded-full text-[10px] font-semibold transition-colors ${
                  isActive
                    ? tab.badgeActiveStyle
                    : "bg-neutral-800/90 text-neutral-400 group-hover:text-neutral-300"
                }`}
              >
                {tab.count}
              </span>
            </button>
          );
        })}
      </div>

      {/* Right Action: Clear Completed */}
      {counts.completed > 0 ? (
        <button
          type="button"
          onClick={onClearCompleted}
          className="text-xs text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800/60 px-3 py-1.5 rounded-lg flex items-center gap-1.5 transition-all cursor-pointer border border-transparent hover:border-neutral-700/60"
          title="Clear completed tasks from list"
        >
          <Trash2 className="w-3.5 h-3.5 text-neutral-400" />
          <span>Clear Completed</span>
        </button>
      ) : (
        <button
          type="button"
          disabled
          className="text-xs text-neutral-600 px-3 py-1.5 rounded-lg flex items-center gap-1.5 border border-transparent cursor-not-allowed opacity-50"
          title="No completed tasks to clear"
        >
          <Trash2 className="w-3.5 h-3.5 text-neutral-600" />
          <span>Clear Completed</span>
        </button>
      )}
    </div>
  );
};
