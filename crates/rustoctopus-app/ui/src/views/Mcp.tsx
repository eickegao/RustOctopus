import { useState, useEffect } from "react";
import { api, McpServerStatus } from "../lib/invoke";

export default function Mcp() {
  const [servers, setServers] = useState<McpServerStatus[]>([]);
  const [showAdd, setShowAdd] = useState(false);
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [env, setEnv] = useState("");
  const [error, setError] = useState("");
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);

  const loadServers = async () => {
    try {
      const list = await api.listMcpServers();
      setServers(list);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    loadServers();
    const interval = setInterval(loadServers, 2000);
    return () => clearInterval(interval);
  }, []);

  const handleAdd = async () => {
    if (!name.trim() || !command.trim()) {
      setError("Name and command are required");
      return;
    }
    const argsList = args
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);
    const envMap: Record<string, string> = {};
    for (const line of env.split("\n")) {
      const eq = line.indexOf("=");
      if (eq > 0) envMap[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
    }
    try {
      await api.addMcpServer(name.trim(), command.trim(), argsList, envMap);
      setShowAdd(false);
      setName("");
      setCommand("");
      setArgs("");
      setEnv("");
      setError("");
      loadServers();
    } catch (e: unknown) {
      setError(String(e));
    }
  };

  const handleRemove = async (serverName: string) => {
    try {
      await api.removeMcpServer(serverName);
    } catch (e) {
      console.error(e);
    }
    setConfirmRemove(null);
    loadServers();
  };

  const handleToggle = async (serverName: string, enabled: boolean) => {
    try {
      await api.toggleMcpServer(serverName, enabled);
    } catch (e) {
      console.error(e);
    }
    loadServers();
  };

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">MCP Servers</h1>
        <button
          onClick={() => {
            setShowAdd(!showAdd);
            setError("");
          }}
          className="px-3 py-1.5 rounded-md bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
        >
          {showAdd ? "Cancel" : "+ Add Server"}
        </button>
      </div>

      {showAdd && (
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4 space-y-3">
          <input
            className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm"
            placeholder="Server name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <input
            className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm"
            placeholder="Command (e.g. npx, uvx, docker)"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
          />
          <textarea
            className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm font-mono"
            placeholder="Arguments (one per line)"
            rows={3}
            value={args}
            onChange={(e) => setArgs(e.target.value)}
          />
          <textarea
            className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm font-mono"
            placeholder="Environment variables (KEY=VALUE, one per line)"
            rows={2}
            value={env}
            onChange={(e) => setEnv(e.target.value)}
          />
          {error && (
            <div className="text-sm text-red-500 dark:text-red-400">
              {error}
            </div>
          )}
          <button
            onClick={handleAdd}
            className="px-4 py-2 rounded-md bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
          >
            Add
          </button>
        </div>
      )}

      {servers.length === 0 ? (
        <div className="text-gray-400 text-sm">
          No MCP servers configured.
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {servers.map((server) => (
            <div
              key={server.name}
              className="rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4"
            >
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <span
                    className={`h-2.5 w-2.5 rounded-full ${
                      server.error
                        ? "bg-red-400"
                        : server.running
                          ? "bg-green-400"
                          : "bg-gray-300 dark:bg-gray-600"
                    }`}
                  />
                  <span className="text-base font-semibold">{server.name}</span>
                </div>
                <span
                  className={`text-xs font-medium px-2 py-0.5 rounded-full ${
                    server.error
                      ? "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400"
                      : server.running
                        ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400"
                        : "bg-gray-100 text-gray-500 dark:bg-gray-700 dark:text-gray-400"
                  }`}
                >
                  {server.error ? "Error" : server.running ? "Running" : "Stopped"}
                </span>
              </div>

              <div className="space-y-1 mb-3">
                <div className="flex text-sm">
                  <span className="w-24 text-gray-500 dark:text-gray-400">
                    Tools
                  </span>
                  <span className="text-gray-700 dark:text-gray-300">
                    {server.tool_count}
                  </span>
                </div>
              </div>

              {server.error && (
                <div className="mb-3 text-xs text-red-500 dark:text-red-400 bg-red-50 dark:bg-red-900/20 rounded-md px-3 py-2 break-words">
                  {server.error}
                </div>
              )}

              <div className="flex items-center justify-between border-t border-gray-100 dark:border-gray-700 pt-3">
                <button
                  onClick={() => handleToggle(server.name, !server.enabled)}
                  className={`text-xs font-medium ${
                    server.enabled
                      ? "text-indigo-600 hover:text-indigo-800 dark:text-indigo-400"
                      : "text-gray-500 hover:text-gray-700 dark:text-gray-400"
                  }`}
                >
                  {server.enabled ? "Disable" : "Enable"}
                </button>

                {confirmRemove === server.name ? (
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-gray-500">Remove?</span>
                    <button
                      onClick={() => handleRemove(server.name)}
                      className="text-xs font-medium text-red-500 hover:text-red-700"
                    >
                      Yes
                    </button>
                    <button
                      onClick={() => setConfirmRemove(null)}
                      className="text-xs font-medium text-gray-500 hover:text-gray-700 dark:text-gray-400"
                    >
                      No
                    </button>
                  </div>
                ) : (
                  <button
                    onClick={() => setConfirmRemove(server.name)}
                    className="text-xs text-red-500 hover:text-red-700"
                  >
                    Remove
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
