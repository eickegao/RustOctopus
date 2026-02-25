import { useEffect, useState } from "react";
import { api, StatusInfo } from "../lib/invoke";
import StatusCard from "../components/StatusCard";

function formatUptime(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  return h > 0 ? `${h}h ${m}m` : m > 0 ? `${m}m ${s}s` : `${s}s`;
}

export default function Dashboard() {
  const [status, setStatus] = useState<StatusInfo | null>(null);

  useEffect(() => {
    const load = () => api.getStatus().then(setStatus).catch(console.error);
    load();
    const interval = setInterval(load, 5000);
    return () => clearInterval(interval);
  }, []);

  if (!status) return <div className="p-6">Loading...</div>;

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-2xl font-bold">Dashboard</h1>

      <div className="grid grid-cols-3 gap-4">
        <StatusCard label="Model" value={status.model} />
        <StatusCard label="Uptime" value={formatUptime(status.uptimeSecs)} />
        <StatusCard
          label="Cron Jobs"
          value={status.cronEnabledCount}
          sub={`${status.cronJobCount} total`}
        />
      </div>

      <div>
        <h2 className="text-lg font-semibold mb-3">Channels</h2>
        <div className="space-y-2">
          {status.channels.length === 0 ? (
            <div className="text-gray-400 text-sm">No channels active</div>
          ) : (
            status.channels.map((ch) => (
              <div
                key={ch}
                className="flex items-center gap-2 px-3 py-2 rounded-md bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700"
              >
                <span className="h-2 w-2 rounded-full bg-green-400" />
                <span className="text-sm font-medium">{ch}</span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
