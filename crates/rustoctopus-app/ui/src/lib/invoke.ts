import { invoke } from "@tauri-apps/api/core";

export interface StatusInfo {
  model: string;
  uptimeSecs: number;
  channels: string[];
  cronJobCount: number;
  cronEnabledCount: number;
  cronNextFireMs: number | null;
}

export interface ChannelInfo {
  name: string;
  enabled: boolean;
}

export interface CronJob {
  id: string;
  name: string;
  enabled: boolean;
  schedule: {
    kind: "at" | "every" | "cron";
    atMs: number | null;
    everyMs: number | null;
    expr: string | null;
    tz: string | null;
  };
  payload: {
    kind: string;
    message: string;
    deliver: boolean;
    channel: string | null;
    to: string | null;
  };
  state: {
    nextRunAtMs: number | null;
    lastRunAtMs: number | null;
    lastStatus: string | null;
    lastError: string | null;
  };
  createdAtMs: number;
  updatedAtMs: number;
  deleteAfterRun: boolean;
}

export interface Config {
  agents: { defaults: AgentDefaults };
  channels: ChannelsConfig;
  providers: Record<string, ProviderConfig>;
  gateway: { host: string; port: number };
  tools: ToolsConfig;
}

export interface AgentDefaults {
  workspace: string;
  model: string;
  maxTokens: number;
  temperature: number;
  maxToolIterations: number;
  memoryWindow: number;
}

export interface ChannelsConfig {
  sendProgress: boolean;
  sendToolHints: boolean;
  telegram: { enabled: boolean; token: string; allowFrom: string[]; proxy: string | null; replyToMessage: boolean };
  feishu: { enabled: boolean; appId: string; appSecret: string; encryptKey: string; verificationToken: string; allowFrom: string[] };
  whatsapp: { enabled: boolean; allowFrom: string[]; bridgePort: number; bridgeToken: string | null; autoStartBridge: boolean };
}

export interface ProviderConfig {
  apiKey: string;
  apiBase: string | null;
  extraHeaders: Record<string, string> | null;
}

export interface ToolsConfig {
  web: { search: { apiKey: string; maxResults: number } };
  exec: { timeout: number };
  restrictToWorkspace: boolean;
}

export interface McpServerStatus {
  name: string;
  enabled: boolean;
  running: boolean;
  tool_count: number;
  error?: string;
}

export const api = {
  getStatus: () => invoke<StatusInfo>("get_status"),
  getConfig: () => invoke<Config>("get_config"),
  saveConfig: (config: Config) => invoke<void>("save_config_cmd", { config }),
  getChannelStatus: () => invoke<ChannelInfo[]>("get_channel_status"),
  listCronJobs: () => invoke<CronJob[]>("list_cron_jobs"),
  addCronJob: (req: { name: string; message: string; scheduleKind: string; everyMs?: number; cronExpr?: string }) =>
    invoke<CronJob>("add_cron_job", { req }),
  removeCronJob: (jobId: string) => invoke<boolean>("remove_cron_job", { jobId }),
  toggleCronJob: (jobId: string) => invoke<boolean>("toggle_cron_job", { jobId }),
  listMcpServers: () => invoke<McpServerStatus[]>("list_mcp_servers"),
  addMcpServer: (name: string, command: string, args: string[], env: Record<string, string>) =>
    invoke<void>("add_mcp_server", { name, command, args, env }),
  removeMcpServer: (name: string) => invoke<void>("remove_mcp_server", { name }),
  toggleMcpServer: (name: string, enabled: boolean) =>
    invoke<void>("toggle_mcp_server", { name, enabled }),
};
