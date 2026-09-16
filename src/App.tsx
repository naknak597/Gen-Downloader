import { useState, useMemo } from "react";
import { Download, Layers, CheckCircle2, AlertCircle } from "lucide-react";
import { Header } from "./components/Header";
import { DownloadForm } from "./components/DownloadForm";
import { TaskCard } from "./components/TaskCard";
import { TaskFilters } from "./components/TaskFilters";
import { useDownload } from "./hooks/useDownload";
import { TaskFilter } from "./types/download";

export default function App() {
  const [currentFilter, setCurrentFilter] = useState<TaskFilter>("all");
  const {
    tasks,
    isSubmitting,
    startDownload,
    cancelDownload,
    removeTask,
    clearCompleted,
  } = useDownload();

  // Optimized task counts for segmented badges
  const counts = useMemo(() => {
    let active = 0;
    let completed = 0;
    let failed = 0;

    for (const t of tasks) {
      if (t.status === "Downloading" || t.status === "Processing/GPU") {
        active++;
      } else if (t.status === "Completed") {
        completed++;
      } else if (t.status === "Error" || t.status === "Cancelled") {
        failed++;
      }
    }

    return {
      all: tasks.length,
      active,
      completed,
      failed,
    };
  }, [tasks]);

  // Memoized task filtering based on current category
  const filteredTasks = useMemo(() => {
    switch (currentFilter) {
      case "active":
        return tasks.filter(
          (t) => t.status === "Downloading" || t.status === "Processing/GPU"
        );
      case "completed":
        return tasks.filter((t) => t.status === "Completed");
      case "failed":
        return tasks.filter(
          (t) => t.status === "Error" || t.status === "Cancelled"
        );
      case "all":
      default:
        return tasks;
    }
  }, [tasks, currentFilter]);

  const renderEmptyState = () => {
    switch (currentFilter) {
      case "active":
        return (
          <div className="flex flex-col items-center justify-center h-full text-neutral-500 py-12">
            <Layers className="w-10 h-10 mb-3 stroke-[1.25] text-cyan-500/60" />
            <p className="text-sm font-medium text-neutral-300">No active downloads</p>
            <p className="text-xs text-neutral-600 mt-1">
              Currently running downloads and conversions will appear here
            </p>
          </div>
        );
      case "completed":
        return (
          <div className="flex flex-col items-center justify-center h-full text-neutral-500 py-12">
            <CheckCircle2 className="w-10 h-10 mb-3 stroke-[1.25] text-emerald-500/60" />
            <p className="text-sm font-medium text-neutral-300">No completed tasks</p>
            <p className="text-xs text-neutral-600 mt-1">
              Successfully finished media files will be listed here
            </p>
          </div>
        );
      case "failed":
        return (
          <div className="flex flex-col items-center justify-center h-full text-neutral-500 py-12">
            <AlertCircle className="w-10 h-10 mb-3 stroke-[1.25] text-rose-500/60" />
            <p className="text-sm font-medium text-neutral-300">No failed tasks</p>
            <p className="text-xs text-neutral-600 mt-1">
              Any interrupted or failed downloads will appear here for review
            </p>
          </div>
        );
      case "all":
      default:
        return (
          <div className="flex flex-col items-center justify-center h-full text-neutral-500 py-12">
            <Download className="w-10 h-10 mb-3 stroke-[1.25] text-neutral-600" />
            <p className="text-sm font-medium text-neutral-300">No downloads yet</p>
            <p className="text-xs text-neutral-600 mt-1">
              Paste a link above to start downloading at high speed
            </p>
          </div>
        );
    }
  };

  return (
    <div className="flex flex-col h-screen w-screen bg-neutral-950 text-neutral-100 font-sans select-none overflow-hidden antialiased">
      <Header />
      <main className="flex flex-col flex-1 overflow-hidden p-6 gap-5">
        <DownloadForm onSubmit={startDownload} isSubmitting={isSubmitting} />

        <section className="flex flex-col flex-1 min-h-0 bg-neutral-900/40 border border-neutral-800/80 rounded-2xl overflow-hidden backdrop-blur-sm">
          {/* Segmented Filter Toolbar */}
          <TaskFilters
            activeFilter={currentFilter}
            onFilterChange={setCurrentFilter}
            counts={counts}
            onClearCompleted={clearCompleted}
          />

          {/* Task List */}
          <div className="flex-1 overflow-y-auto p-4 space-y-3">
            {filteredTasks.length === 0 ? (
              renderEmptyState()
            ) : (
              filteredTasks.map((task) => (
                <TaskCard
                  key={task.taskId}
                  task={task}
                  onCancel={cancelDownload}
                  onRemove={removeTask}
                />
              ))
            )}
          </div>
        </section>
      </main>
    </div>
  );
}
