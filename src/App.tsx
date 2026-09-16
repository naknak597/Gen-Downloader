import { useState } from "react";
import { Download, Trash2 } from "lucide-react";
import { Header } from "./components/Header";
import { DownloadForm } from "./components/DownloadForm";
import { TaskCard } from "./components/TaskCard";
import { useDownload } from "./hooks/useDownload";
import { FilterTab } from "./types/download";

export default function App() {
  const [activeTab, setActiveTab] = useState<FilterTab>("all");
  const {
    tasks,
    isSubmitting,
    startDownload,
    cancelDownload,
    removeTask,
    clearCompleted,
    activeCount,
    completedCount,
  } = useDownload();

  const filteredTasks = tasks.filter((t) => {
    if (activeTab === "active") return t.status === "Downloading" || t.status === "Processing/GPU";
    if (activeTab === "completed") return t.status === "Completed";
    return true;
  });

  return (
    <div className="flex flex-col h-screen w-screen bg-neutral-950 text-neutral-100 font-sans select-none overflow-hidden antialiased">
      <Header />
      <main className="flex flex-col flex-1 overflow-hidden p-6 gap-5">
        <DownloadForm onSubmit={startDownload} isSubmitting={isSubmitting} />

        <section className="flex flex-col flex-1 min-h-0 bg-neutral-900/40 border border-neutral-800/80 rounded-2xl overflow-hidden backdrop-blur-sm">
          <div className="flex items-center justify-between px-5 py-3 border-b border-neutral-800/80 shrink-0 bg-neutral-900/60">
            <div className="flex items-center gap-2">
              <button onClick={() => setActiveTab("all")} className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer ${activeTab === "all" ? "bg-neutral-800 text-neutral-100" : "text-neutral-400 hover:text-neutral-200"}`}>All Tasks ({tasks.length})</button>
              <button onClick={() => setActiveTab("active")} className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer ${activeTab === "active" ? "bg-cyan-500/10 text-cyan-400 border border-cyan-500/20" : "text-neutral-400 hover:text-neutral-200"}`}>Active ({activeCount})</button>
              <button onClick={() => setActiveTab("completed")} className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer ${activeTab === "completed" ? "bg-emerald-500/10 text-emerald-400 border border-emerald-500/20" : "text-neutral-400 hover:text-neutral-200"}`}>Completed ({completedCount})</button>
            </div>
            {completedCount > 0 && (
              <button onClick={clearCompleted} className="text-xs text-neutral-400 hover:text-neutral-200 flex items-center gap-1.5 transition-colors cursor-pointer">
                <Trash2 className="w-3.5 h-3.5" /><span>Clear Completed</span>
              </button>
            )}
          </div>

          <div className="flex-1 overflow-y-auto p-4 space-y-3">
            {filteredTasks.length === 0 ? (
              <div className="flex flex-col items-center justify-center h-full text-neutral-500 py-12">
                <Download className="w-10 h-10 mb-3 stroke-[1.25] text-neutral-600" />
                <p className="text-sm font-medium">No downloads in this view</p>
                <p className="text-xs text-neutral-600 mt-1">Paste a link above to start downloading at high speed</p>
              </div>
            ) : (
              filteredTasks.map((task) => (
                <TaskCard key={task.taskId} task={task} onCancel={cancelDownload} onRemove={removeTask} />
              ))
            )}
          </div>
        </section>
      </main>
    </div>
  );
}
