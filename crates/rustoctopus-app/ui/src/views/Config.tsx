import { useEffect, useState } from "react";
import { api, Config } from "../lib/invoke";

export default function ConfigView() {
  const [config, setConfig] = useState<Config | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api.getConfig().then(setConfig).catch(console.error);
  }, []);

  if (!config) return <div className="p-6">Loading...</div>;

  const update = (fn: (c: Config) => void) => {
    const next = structuredClone(config);
    fn(next);
    setConfig(next);
    setSaved(false);
  };

  const handleSave = async () => {
    if (!config) return;
    setSaving(true);
    try {
      await api.saveConfig(config);
      setSaved(true);
    } catch (e) {
      console.error(e);
      alert("Save failed: " + e);
    }
    setSaving(false);
  };

  const d = config.agents.defaults;

  return (
    <div className="p-6 space-y-8 max-w-2xl">
      <h1 className="text-2xl font-bold">Configuration</h1>

      <section className="space-y-4">
        <h2 className="text-lg font-semibold">Agent</h2>
        <Field label="Model" value={d.model} onChange={(v) => update((c) => (c.agents.defaults.model = v))} />
        <Field label="Max Tokens" value={String(d.maxTokens)} onChange={(v) => update((c) => (c.agents.defaults.maxTokens = Number(v)))} type="number" />
        <RangeField label="Temperature" value={d.temperature} min={0} max={2} step={0.1} onChange={(v) => update((c) => (c.agents.defaults.temperature = v))} />
        <Field label="Memory Window" value={String(d.memoryWindow)} onChange={(v) => update((c) => (c.agents.defaults.memoryWindow = Number(v)))} type="number" />
      </section>

      <section className="space-y-4">
        <h2 className="text-lg font-semibold">Providers</h2>
        {Object.entries(config.providers).map(([name, prov]) => (
          <Field
            key={name}
            label={name}
            value={prov.apiKey}
            onChange={(v) => update((c) => ((c.providers as Record<string, { apiKey: string }>)[name].apiKey = v))}
            type="password"
            placeholder="API Key"
          />
        ))}
      </section>

      <section className="space-y-4">
        <h2 className="text-lg font-semibold">Tools</h2>
        <Field label="Exec Timeout (sec)" value={String(config.tools.exec.timeout)} onChange={(v) => update((c) => (c.tools.exec.timeout = Number(v)))} type="number" />
        <Field label="Web Search API Key" value={config.tools.web.search.apiKey} onChange={(v) => update((c) => (c.tools.web.search.apiKey = v))} type="password" />
      </section>

      <button
        onClick={handleSave}
        disabled={saving}
        className="px-4 py-2 rounded-md bg-indigo-600 text-white font-medium hover:bg-indigo-700 disabled:opacity-50 transition-colors"
      >
        {saving ? "Saving..." : saved ? "Saved!" : "Save"}
      </button>
    </div>
  );
}

function Field({ label, value, onChange, type = "text", placeholder }: {
  label: string; value: string; onChange: (v: string) => void; type?: string; placeholder?: string;
}) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-600 dark:text-gray-400 mb-1">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm focus:ring-2 focus:ring-indigo-500 focus:border-transparent outline-none"
      />
    </div>
  );
}

function RangeField({ label, value, min, max, step, onChange }: {
  label: string; value: number; min: number; max: number; step: number; onChange: (v: number) => void;
}) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-600 dark:text-gray-400 mb-1">
        {label}: {value}
      </label>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full"
      />
    </div>
  );
}
