import { useEffect, useState } from "react";
import { api, CronJob } from "../lib/invoke";

function formatSchedule(job: CronJob): string {
  const s = job.schedule;
  if (s.kind === "every" && s.everyMs) {
    const sec = s.everyMs / 1000;
    if (sec < 60) return `Every ${sec}s`;
    if (sec < 3600) return `Every ${Math.round(sec / 60)}m`;
    return `Every ${Math.round(sec / 3600)}h`;
  }
  if (s.kind === "cron" && s.expr) return s.expr;
  if (s.kind === "at" && s.atMs) return new Date(s.atMs).toLocaleString();
  return "unknown";
}

function formatTime(ms: number | null): string {
  if (!ms) return "-";
  return new Date(ms).toLocaleString();
}

export default function Cron() {
  const [jobs, setJobs] = useState<CronJob[]>([]);
  const [showAdd, setShowAdd] = useState(false);
  const [name, setName] = useState("");
  const [message, setMessage] = useState("");
  const [scheduleKind, setScheduleKind] = useState("every");
  const [everyMin, setEveryMin] = useState("30");
  const [cronExpr, setCronExpr] = useState("0 0 9 * * *");

  const load = () => api.listCronJobs().then(setJobs).catch(console.error);

  useEffect(() => { load(); }, []);

  const handleAdd = async () => {
    try {
      await api.addCronJob({
        name,
        message,
        scheduleKind,
        everyMs: scheduleKind === "every" ? Number(everyMin) * 60 * 1000 : undefined,
        cronExpr: scheduleKind === "cron" ? cronExpr : undefined,
      });
      setShowAdd(false);
      setName("");
      setMessage("");
      load();
    } catch (e) {
      alert("Failed to add job: " + e);
    }
  };

  const handleRemove = async (id: string) => {
    if (!confirm("Remove this job?")) return;
    await api.removeCronJob(id);
    load();
  };

  const handleToggle = async (id: string) => {
    await api.toggleCronJob(id);
    load();
  };

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Cron Jobs</h1>
        <button
          onClick={() => setShowAdd(!showAdd)}
          className="px-3 py-1.5 rounded-md bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
        >
          {showAdd ? "Cancel" : "Add Job"}
        </button>
      </div>

      {showAdd && (
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4 space-y-3">
          <input className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" placeholder="Job name" value={name} onChange={(e) => setName(e.target.value)} />
          <textarea className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" placeholder="Message" rows={2} value={message} onChange={(e) => setMessage(e.target.value)} />
          <div className="flex gap-3 items-center">
            <select className="px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" value={scheduleKind} onChange={(e) => setScheduleKind(e.target.value)}>
              <option value="every">Every N minutes</option>
              <option value="cron">Cron expression</option>
            </select>
            {scheduleKind === "every" ? (
              <input className="w-24 px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" type="number" value={everyMin} onChange={(e) => setEveryMin(e.target.value)} placeholder="min" />
            ) : (
              <input className="flex-1 px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm" value={cronExpr} onChange={(e) => setCronExpr(e.target.value)} placeholder="0 0 9 * * *" />
            )}
          </div>
          <button onClick={handleAdd} className="px-4 py-2 rounded-md bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors">
            Add
          </button>
        </div>
      )}

      {jobs.length === 0 ? (
        <div className="text-gray-400 text-sm">No cron jobs configured.</div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-sm text-left">
            <thead className="text-xs text-gray-500 dark:text-gray-400 uppercase border-b border-gray-200 dark:border-gray-700">
              <tr>
                <th className="py-2 pr-4">Name</th>
                <th className="py-2 pr-4">Schedule</th>
                <th className="py-2 pr-4">Last Run</th>
                <th className="py-2 pr-4">Next Run</th>
                <th className="py-2 pr-4">Status</th>
                <th className="py-2">Actions</th>
              </tr>
            </thead>
            <tbody>
              {jobs.map((job) => (
                <tr key={job.id} className="border-b border-gray-100 dark:border-gray-800">
                  <td className="py-2 pr-4 font-medium">{job.name}</td>
                  <td className="py-2 pr-4 font-mono text-xs">{formatSchedule(job)}</td>
                  <td className="py-2 pr-4 text-gray-500">{formatTime(job.state.lastRunAtMs)}</td>
                  <td className="py-2 pr-4 text-gray-500">{formatTime(job.state.nextRunAtMs)}</td>
                  <td className="py-2 pr-4">
                    <span className={`text-xs font-medium px-2 py-0.5 rounded-full ${job.enabled ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400" : "bg-gray-100 text-gray-500 dark:bg-gray-700 dark:text-gray-400"}`}>
                      {job.enabled ? "Active" : "Disabled"}
                    </span>
                  </td>
                  <td className="py-2 space-x-2">
                    <button onClick={() => handleToggle(job.id)} className="text-xs text-indigo-600 hover:text-indigo-800 dark:text-indigo-400">
                      {job.enabled ? "Disable" : "Enable"}
                    </button>
                    <button onClick={() => handleRemove(job.id)} className="text-xs text-red-500 hover:text-red-700">
                      Remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
