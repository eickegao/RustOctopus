import { useState, useEffect, useRef, useCallback } from "react";
import { api, McpServerStatus, RegistryServer } from "../lib/invoke";

export default function Mcp() {
  const [servers, setServers] = useState<McpServerStatus[]>([]);
  const [showAdd, setShowAdd] = useState(false);
  const [editTarget, setEditTarget] = useState<McpServerStatus | null>(null);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);

  // Form state
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [env, setEnv] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [error, setError] = useState("");

  // Registry search state
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<RegistryServer[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchDone, setSearchDone] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setSearchResults([]);
      setSearching(false);
      setSearchDone(false);
      return;
    }
    setSearching(true);
    try {
      const res = await api.searchMcpRegistry(q.trim(), 20);
      setSearchResults(res.servers);
      setSearchDone(true);
    } catch {
      setSearchResults([]);
      setSearchDone(true);
    } finally {
      setSearching(false);
    }
  }, []);

  const onSearchInput = (value: string) => {
    setSearchQuery(value);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => doSearch(value), 300);
  };

  const shortName = (fullName: string) => {
    const idx = fullName.lastIndexOf("/");
    return idx >= 0 ? fullName.slice(idx + 1) : fullName;
  };

  const selectRegistryServer = (server: RegistryServer) => {
    setName(shortName(server.name));
    const pkg = server.packages[0];
    if (pkg) {
      const rt = pkg.registry_type;
      if (rt === "npm") {
        setCommand("npx");
        setArgs(pkg.identifier ? `-y\n${pkg.identifier}` : "");
      } else if (rt === "pypi") {
        setCommand("uvx");
        setArgs(pkg.identifier ?? "");
      } else {
        setCommand("");
        setArgs(pkg.identifier ?? "");
      }
      const envLines = pkg.environment_variables
        .filter((ev) => ev.is_required)
        .map((ev) => `${ev.name}=`)
        .join("\n");
      setEnv(envLines);
    } else {
      setCommand("");
      setArgs("");
      setEnv("");
    }
  };

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

  const resetForm = () => {
    setName("");
    setCommand("");
    setArgs("");
    setEnv("");
    setEnabled(true);
    setError("");
    setEditTarget(null);
    setSearchQuery("");
    setSearchResults([]);
    setSearchDone(false);
  };

  const openAdd = () => {
    resetForm();
    setShowAdd(true);
  };

  const closeModal = () => {
    setShowAdd(false);
    resetForm();
  };

  const handleSubmit = async () => {
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
      if (editTarget) {
        // Remove old then re-add (no update API)
        await api.removeMcpServer(editTarget.name);
      }
      await api.addMcpServer(name.trim(), command.trim(), argsList, envMap);
      if (!enabled) {
        await api.toggleMcpServer(name.trim(), false);
      }
      closeModal();
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

  const handleToggle = async (serverName: string, currentEnabled: boolean) => {
    try {
      await api.toggleMcpServer(serverName, !currentEnabled);
    } catch (e) {
      console.error(e);
    }
    loadServers();
  };

  const openEdit = (server: McpServerStatus) => {
    setEditTarget(server);
    setName(server.name);
    setCommand("");
    setArgs("");
    setEnv("");
    setEnabled(server.enabled);
    setError("");
    setSearchQuery("");
    setSearchResults([]);
    setSearchDone(false);
    setShowAdd(true);
  };

  const statusBadge = (server: McpServerStatus) => {
    if (server.error) {
      return (
        <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400">
          Error
        </span>
      );
    }
    if (server.running) {
      return (
        <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400">
          Running
        </span>
      );
    }
    return (
      <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-gray-100 text-gray-500 dark:bg-gray-700 dark:text-gray-400">
        Stopped
      </span>
    );
  };

  return (
    <div className="p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Installed Apps</h1>
        <button
          onClick={loadServers}
          className="px-3 py-1.5 rounded-md border border-gray-300 dark:border-gray-600 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
        >
          Refresh
        </button>
      </div>

      {/* Card grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Add new card */}
        <button
          onClick={openAdd}
          className="flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed border-gray-300 dark:border-gray-600 p-6 min-h-[160px] text-gray-400 dark:text-gray-500 hover:border-indigo-400 hover:text-indigo-500 dark:hover:border-indigo-500 dark:hover:text-indigo-400 transition-colors cursor-pointer"
        >
          <svg className="w-10 h-10" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
          </svg>
          <span className="text-sm font-medium">Add New App</span>
        </button>

        {/* Server cards */}
        {servers.map((server) => (
          <div
            key={server.name}
            className="flex flex-col rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-4 min-h-[160px]"
          >
            {/* Top: name + status */}
            <div className="flex items-center justify-between mb-2">
              <span className="text-sm font-semibold truncate mr-2">{server.name}</span>
              {statusBadge(server)}
            </div>

            {/* Description */}
            <p className="text-xs text-gray-500 dark:text-gray-400 mb-1 flex-1">
              {server.tool_count} tool{server.tool_count !== 1 ? "s" : ""} available
            </p>

            {server.error && (
              <p className="text-xs text-red-500 dark:text-red-400 bg-red-50 dark:bg-red-900/20 rounded px-2 py-1 mb-2 break-words line-clamp-2">
                {server.error}
              </p>
            )}

            {/* Bottom actions */}
            <div className="flex items-center justify-between border-t border-gray-100 dark:border-gray-700 pt-3 mt-auto">
              <label className="flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  checked={server.enabled}
                  onChange={() => handleToggle(server.name, server.enabled)}
                  className="h-3.5 w-3.5 rounded border-gray-300 text-indigo-600 focus:ring-indigo-500"
                />
                <span className="text-xs text-gray-500 dark:text-gray-400">Enabled</span>
              </label>
              <div className="flex items-center gap-2">
                {/* Edit */}
                <button
                  onClick={() => openEdit(server)}
                  className="text-gray-400 hover:text-indigo-500 dark:hover:text-indigo-400 transition-colors"
                  title="Edit"
                >
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="m16.862 4.487 1.687-1.688a1.875 1.875 0 1 1 2.652 2.652L10.582 16.07a4.5 4.5 0 0 1-1.897 1.13L6 18l.8-2.685a4.5 4.5 0 0 1 1.13-1.897l8.932-8.931Z" />
                    <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 7.125 16.862 4.487" />
                  </svg>
                </button>
                {/* Delete */}
                <button
                  onClick={() => setConfirmRemove(server.name)}
                  className="text-gray-400 hover:text-red-500 dark:hover:text-red-400 transition-colors"
                  title="Delete"
                >
                  <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="m14.74 9-.346 9m-4.788 0L9.26 9m9.968-3.21c.342.052.682.107 1.022.166m-1.022-.165L18.16 19.673a2.25 2.25 0 0 1-2.244 2.077H8.084a2.25 2.25 0 0 1-2.244-2.077L4.772 5.79m14.456 0a48.108 48.108 0 0 0-3.478-.397m-12 .562c.34-.059.68-.114 1.022-.165m0 0a48.11 48.11 0 0 1 3.478-.397m7.5 0v-.916c0-1.18-.91-2.164-2.09-2.201a51.964 51.964 0 0 0-3.32 0c-1.18.037-2.09 1.022-2.09 2.201v.916m7.5 0a48.667 48.667 0 0 0-7.5 0" />
                  </svg>
                </button>
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* Delete confirmation modal */}
      {confirmRemove && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={() => setConfirmRemove(null)}>
          <div
            className="bg-white dark:bg-gray-800 rounded-lg shadow-xl p-6 max-w-sm w-full mx-4"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-base font-semibold mb-2">Confirm Delete</h3>
            <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">
              Are you sure you want to delete <span className="font-medium text-gray-700 dark:text-gray-200">{confirmRemove}</span>? This action cannot be undone.
            </p>
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setConfirmRemove(null)}
                className="px-3 py-1.5 rounded-md border border-gray-300 dark:border-gray-600 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={() => handleRemove(confirmRemove)}
                className="px-3 py-1.5 rounded-md bg-red-600 text-white text-sm font-medium hover:bg-red-700 transition-colors"
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Add/Edit modal */}
      {showAdd && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={closeModal}>
          <div
            className="bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-4xl w-full mx-4 max-h-[85vh] overflow-y-auto"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="p-6">
              <h2 className="text-lg font-semibold mb-4">
                {editTarget ? "Edit MCP App" : "Add New MCP App"}
              </h2>

              <div className="flex flex-col lg:flex-row gap-6">
                {/* Left: Registry search */}
                <div className="lg:w-1/2 space-y-4">
                  <div>
                    <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                      Search MCP Registry
                    </label>
                    <div className="relative">
                      <input
                        className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-900 text-sm"
                        placeholder="Search servers..."
                        value={searchQuery}
                        onChange={(e) => onSearchInput(e.target.value)}
                      />
                      {searching && (
                        <div className="absolute right-3 top-1/2 -translate-y-1/2">
                          <svg className="w-4 h-4 animate-spin text-gray-400" viewBox="0 0 24 24" fill="none">
                            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                          </svg>
                        </div>
                      )}
                    </div>
                  </div>
                  <div className="rounded-md border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900 min-h-[280px] max-h-[360px] overflow-y-auto">
                    {!searchQuery.trim() && !searchDone && (
                      <div className="flex items-center justify-center h-[280px] text-sm text-gray-400 dark:text-gray-500">
                        Search the MCP registry to find servers
                      </div>
                    )}
                    {searchDone && searchResults.length === 0 && (
                      <div className="flex items-center justify-center h-[280px] text-sm text-gray-400 dark:text-gray-500">
                        No servers found
                      </div>
                    )}
                    {searchResults.map((srv) => (
                      <button
                        key={srv.name}
                        type="button"
                        onClick={() => selectRegistryServer(srv)}
                        className="w-full text-left px-3 py-2.5 border-b border-gray-200 dark:border-gray-700 last:border-b-0 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                      >
                        <div className="flex items-center gap-2 mb-0.5">
                          <span className="text-sm font-medium truncate">{shortName(srv.name)}</span>
                          {srv.version && (
                            <span className="shrink-0 inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-gray-200 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
                              v{srv.version}
                            </span>
                          )}
                          {srv.packages[0]?.registry_type && (
                            <span className="shrink-0 inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-indigo-100 dark:bg-indigo-900/30 text-indigo-600 dark:text-indigo-400">
                              {srv.packages[0].registry_type}
                            </span>
                          )}
                        </div>
                        {srv.description && (
                          <p className="text-xs text-gray-500 dark:text-gray-400 line-clamp-2">{srv.description}</p>
                        )}
                      </button>
                    ))}
                  </div>
                </div>

                {/* Right: Form */}
                <div className="lg:w-1/2 space-y-4">
                  <div>
                    <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                      Name
                    </label>
                    <input
                      className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-900 text-sm"
                      placeholder="my-mcp-server"
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                    />
                  </div>

                  <div>
                    <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                      Transport
                    </label>
                    <select className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-900 text-sm">
                      <option value="stdio">stdio</option>
                    </select>
                  </div>

                  <div>
                    <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                      Command
                    </label>
                    <input
                      className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-900 text-sm"
                      placeholder="npx, uvx, docker ..."
                      value={command}
                      onChange={(e) => setCommand(e.target.value)}
                    />
                  </div>

                  <div>
                    <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                      Arguments <span className="text-gray-400 font-normal">(one per line)</span>
                    </label>
                    <textarea
                      className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-900 text-sm font-mono"
                      placeholder={"-y\n@modelcontextprotocol/server-github"}
                      rows={3}
                      value={args}
                      onChange={(e) => setArgs(e.target.value)}
                    />
                  </div>

                  <div>
                    <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                      Environment Variables <span className="text-gray-400 font-normal">(KEY=VALUE, one per line)</span>
                    </label>
                    <textarea
                      className="w-full px-3 py-2 rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-900 text-sm font-mono"
                      placeholder="GITHUB_TOKEN=ghp_xxx"
                      rows={2}
                      value={env}
                      onChange={(e) => setEnv(e.target.value)}
                    />
                  </div>

                  <div className="flex items-center gap-3">
                    <label className="block text-sm font-medium text-gray-700 dark:text-gray-300">
                      Enabled
                    </label>
                    <button
                      type="button"
                      onClick={() => setEnabled(!enabled)}
                      className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none ${
                        enabled ? "bg-indigo-600" : "bg-gray-300 dark:bg-gray-600"
                      }`}
                    >
                      <span
                        className={`pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                          enabled ? "translate-x-4" : "translate-x-0"
                        }`}
                      />
                    </button>
                  </div>
                </div>
              </div>

              {error && (
                <div className="mt-4 text-sm text-red-500 dark:text-red-400 bg-red-50 dark:bg-red-900/20 rounded-md px-3 py-2">
                  {error}
                </div>
              )}

              {/* Footer buttons */}
              <div className="flex justify-end gap-2 mt-6 pt-4 border-t border-gray-200 dark:border-gray-700">
                <button
                  onClick={closeModal}
                  className="px-4 py-2 rounded-md border border-gray-300 dark:border-gray-600 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={handleSubmit}
                  className="px-4 py-2 rounded-md bg-indigo-600 text-white text-sm font-medium hover:bg-indigo-700 transition-colors"
                >
                  {editTarget ? "Save" : "Add"}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
