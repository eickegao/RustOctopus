import { useEffect, useState } from "react";
import { api, ChannelInfo, Config } from "../lib/invoke";

export default function Channels() {
  const [channels, setChannels] = useState<ChannelInfo[]>([]);
  const [config, setConfig] = useState<Config | null>(null);

  useEffect(() => {
    api.getChannelStatus().then(setChannels).catch(console.error);
    api.getConfig().then(setConfig).catch(console.error);
  }, []);

  if (!config) return <div className="p-6">Loading...</div>;

  const channelConfigs: Record<string, { enabled: boolean; fields: { label: string; value: string }[] }> = {
    telegram: {
      enabled: config.channels.telegram.enabled,
      fields: [
        { label: "Token", value: config.channels.telegram.token ? "***configured***" : "Not set" },
        { label: "Allow From", value: config.channels.telegram.allowFrom.join(", ") || "All" },
      ],
    },
    feishu: {
      enabled: config.channels.feishu.enabled,
      fields: [
        { label: "App ID", value: config.channels.feishu.appId || "Not set" },
        { label: "Allow From", value: config.channels.feishu.allowFrom.join(", ") || "All" },
      ],
    },
    whatsapp: {
      enabled: config.channels.whatsapp.enabled,
      fields: [
        { label: "Bridge Port", value: String(config.channels.whatsapp.bridgePort) },
        { label: "Auto Start Bridge", value: config.channels.whatsapp.autoStartBridge ? "Yes" : "No" },
      ],
    },
  };

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-2xl font-bold">Channels</h1>
      <div className="space-y-4">
        {Object.entries(channelConfigs).map(([name, ch]) => {
          const live = channels.find((c) => c.name === name);
          return (
            <div
              key={name}
              className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4"
            >
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <span
                    className={`h-2.5 w-2.5 rounded-full ${
                      live?.enabled ? "bg-green-400" : "bg-gray-300 dark:bg-gray-600"
                    }`}
                  />
                  <span className="text-base font-semibold capitalize">{name}</span>
                </div>
                <span
                  className={`text-xs font-medium px-2 py-0.5 rounded-full ${
                    ch.enabled
                      ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400"
                      : "bg-gray-100 text-gray-500 dark:bg-gray-700 dark:text-gray-400"
                  }`}
                >
                  {ch.enabled ? "Enabled" : "Disabled"}
                </span>
              </div>
              <div className="space-y-1">
                {ch.fields.map((f) => (
                  <div key={f.label} className="flex text-sm">
                    <span className="w-36 text-gray-500 dark:text-gray-400">{f.label}</span>
                    <span className="text-gray-700 dark:text-gray-300">{f.value}</span>
                  </div>
                ))}
              </div>
            </div>
          );
        })}
      </div>
      <p className="text-sm text-gray-400">
        To enable/disable channels, edit settings in the Config page and restart the app.
      </p>
    </div>
  );
}
