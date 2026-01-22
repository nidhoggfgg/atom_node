type Plugin = {
  id: string;
  name: string;
  version: string;
  plugin_type: string;
  description: string;
  author: string;
  entry_point: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  parameters?: PluginParameter[] | null;
};

type Execution = {
  id: string;
  plugin_id: string;
  status: string;
  pid: number | null;
  exit_code: number | null;
  stdout: string | null;
  stderr: string | null;
  started_at: string;
  finished_at: string | null;
  error_message: string | null;
};

type HealthResponse = {
  status: string;
  service: string;
  version: string;
};

type PluginsListResponse = { data: Plugin[] };
type ExecutionsListResponse = { data: Execution[] };

type PluginParameter = {
  name: string;
  type: "string" | "number" | "integer" | "boolean" | "json";
  description?: string | null;
  default?: unknown;
};

type InstallPluginRequest = {
  name: string;
  version: string;
  plugin_type: string;
  description: string;
  author: string;
  package_url: string;
  entry_point: string;
  metadata?: string | null;
  parameters?: PluginParameter[] | null;
};

type ExecutePluginRequest = {
  params?: Record<string, unknown>;
};

const dom = {
  baseUrlInput: document.querySelector<HTMLInputElement>("#base-url")!,
  connectForm: document.querySelector<HTMLFormElement>("#connect-form")!,
  connectBtn: document.querySelector<HTMLButtonElement>("#connect-btn")!,
  statusPill: document.querySelector<HTMLElement>(".status-pill")!,
  statusText: document.querySelector<HTMLElement>("#status-text")!,
  statusMeta: document.querySelector<HTMLElement>("#status-meta")!,
  refreshAll: document.querySelector<HTMLButtonElement>("#refresh-all")!,
  scrollPlugins: document.querySelector<HTMLButtonElement>("#scroll-plugins")!,
  pluginList: document.querySelector<HTMLElement>("#plugin-list")!,
  pluginCount: document.querySelector<HTMLElement>("#plugin-count")!,
  refreshPlugins: document.querySelector<HTMLButtonElement>("#refresh-plugins")!,
  installForm: document.querySelector<HTMLFormElement>("#install-form")!,
  executionForm: document.querySelector<HTMLFormElement>("#execution-form")!,
  executionPluginId: document.querySelector<HTMLInputElement>("#execution-plugin-id")!,
  executionParams: document.querySelector<HTMLTextAreaElement>("#execution-params")!,
  executionList: document.querySelector<HTMLElement>("#execution-list")!,
  executionCount: document.querySelector<HTMLElement>("#execution-count")!,
  refreshExecutions: document.querySelector<HTMLButtonElement>("#refresh-executions")!,
  executionFilterForm: document.querySelector<HTMLFormElement>("#execution-filter-form")!,
  executionFilterInput: document.querySelector<HTMLInputElement>("#execution-filter-id")!,
  executionFilterClear: document.querySelector<HTMLButtonElement>("#execution-filter-clear")!,
  notice: document.querySelector<HTMLElement>("#notice")!,
  lastUpdated: document.querySelector<HTMLElement>("#last-updated")!,
};

const state = {
  baseUrl: localStorage.getItem("atom_node_base_url") || "http://localhost:3000",
  plugins: [] as Plugin[],
  executions: [] as Execution[],
  connected: false,
};

const reducedMotion =
  typeof window.matchMedia === "function" &&
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

let noticeTimer: number | undefined;

const api = {
  async health(): Promise<HealthResponse> {
    return request<HealthResponse>("/health");
  },
  async listPlugins(): Promise<PluginsListResponse> {
    return request<PluginsListResponse>("/api/plugins");
  },
  async installPlugin(payload: InstallPluginRequest): Promise<Plugin> {
    return request<Plugin>("/api/plugins", {
      method: "POST",
      body: JSON.stringify(payload),
    });
  },
  async enablePlugin(id: string): Promise<void> {
    await request<void>(`/api/plugins/${id}/enable`, { method: "PUT" });
  },
  async disablePlugin(id: string): Promise<void> {
    await request<void>(`/api/plugins/${id}/disable`, { method: "PUT" });
  },
  async uninstallPlugin(id: string): Promise<void> {
    await request<void>(`/api/plugins/${id}`, { method: "DELETE" });
  },
  async executePlugin(id: string, payload: ExecutePluginRequest): Promise<Execution> {
    return request<Execution>(`/api/plugins/${id}/execute`, {
      method: "POST",
      body: JSON.stringify(payload),
    });
  },
  async listExecutions(pluginId?: string): Promise<ExecutionsListResponse> {
    const query = pluginId ? `?plugin_id=${encodeURIComponent(pluginId)}` : "";
    return request<ExecutionsListResponse>(`/api/executions${query}`);
  },
  async stopExecution(id: string): Promise<void> {
    await request<void>(`/api/executions/${id}/stop`, { method: "PUT" });
  },
};

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const url = `${state.baseUrl}${path}`;
  const response = await fetch(url, {
    headers: {
      "Content-Type": "application/json",
      ...options.headers,
    },
    ...options,
  });

  if (response.status === 204) {
    if (!response.ok) {
      throw new Error(`Request failed with ${response.status}`);
    }
    return undefined as T;
  }

  const contentType = response.headers.get("content-type") || "";
  const isJson = contentType.includes("application/json");
  const payload = isJson ? await response.json() : await response.text();

  if (!response.ok) {
    const message =
      (payload && typeof payload === "object" && "message" in payload && payload.message) ||
      (payload && typeof payload === "string" ? payload : response.statusText);
    throw new Error(message);
  }

  return payload as T;
}

function normalizeBaseUrl(url: string): string {
  return url.replace(/\/+$/, "");
}

function notify(message: string, variant: "success" | "error" | "info" = "info") {
  dom.notice.textContent = message;
  dom.notice.dataset.variant = variant;
  dom.notice.classList.add("is-visible");
  if (noticeTimer) {
    window.clearTimeout(noticeTimer);
  }
  noticeTimer = window.setTimeout(() => {
    dom.notice.classList.remove("is-visible");
  }, 4200);
}

function updateLastUpdated() {
  const now = new Date();
  dom.lastUpdated.textContent = `Last updated: ${now.toLocaleString()}`;
}

function setConnectionState(connected: boolean, meta?: string) {
  state.connected = connected;
  dom.statusPill.dataset.status = connected ? "online" : "offline";
  dom.statusText.textContent = connected ? "Connected" : "Offline";
  dom.statusMeta.textContent = meta || (connected ? "Connected." : "No connection yet.");
}

async function connect() {
  const baseUrl = normalizeBaseUrl(dom.baseUrlInput.value.trim());
  if (!baseUrl) {
    notify("Base URL is required.", "error");
    return;
  }
  state.baseUrl = baseUrl;
  localStorage.setItem("atom_node_base_url", baseUrl);

  dom.connectBtn.disabled = true;
  dom.connectBtn.textContent = "Connecting...";
  try {
    const health = await api.health();
    const meta = `${health.service} v${health.version} (${health.status})`;
    setConnectionState(true, meta);
    await Promise.all([loadPlugins(), loadExecutions()]);
    notify("Connected to atom_node.", "success");
  } catch (error) {
    const message = error instanceof Error ? error.message : "Connection failed.";
    setConnectionState(false, message);
    notify(message, "error");
  } finally {
    dom.connectBtn.disabled = false;
    dom.connectBtn.textContent = "Connect";
  }
}

async function loadPlugins() {
  const response = await api.listPlugins();
  state.plugins = response.data || [];
  renderPlugins();
  updateLastUpdated();
}

async function loadExecutions() {
  const pluginFilter = dom.executionFilterInput.value.trim();
  const response = await api.listExecutions(pluginFilter || undefined);
  state.executions = response.data || [];
  renderExecutions();
  updateLastUpdated();
}

function renderPlugins() {
  dom.pluginList.innerHTML = "";
  const count = state.plugins.length;
  dom.pluginCount.textContent = `${count} plugin${count === 1 ? "" : "s"}`;

  if (!count) {
    dom.pluginList.append(createEmptyState("No plugins installed yet."));
    return;
  }

  const fragment = document.createDocumentFragment();
  state.plugins.forEach((plugin) => fragment.append(buildPluginCard(plugin)));
  dom.pluginList.append(fragment);
}

function renderExecutions() {
  dom.executionList.innerHTML = "";
  const count = state.executions.length;
  dom.executionCount.textContent = `${count} execution${count === 1 ? "" : "s"}`;

  if (!count) {
    dom.executionList.append(createEmptyState("No executions found."));
    return;
  }

  const fragment = document.createDocumentFragment();
  state.executions.forEach((execution) => fragment.append(buildExecutionCard(execution)));
  dom.executionList.append(fragment);
}

function createEmptyState(message: string) {
  const div = document.createElement("div");
  div.className = "empty-state";
  div.textContent = message;
  return div;
}

function buildPluginCard(plugin: Plugin) {
  const card = document.createElement("article");
  card.className = "item";
  card.dataset.pluginId = plugin.id;

  const enabled = plugin.enabled ? "Enabled" : "Disabled";
  const toggleAction = plugin.enabled ? "disable-plugin" : "enable-plugin";
  const toggleLabel = plugin.enabled ? "Disable" : "Enable";

  card.innerHTML = `
    <div class="item__header">
      <div>
        <h3 title="${escapeHtml(plugin.name)}">${escapeHtml(plugin.name)}</h3>
        <p class="meta">${escapeHtml(plugin.version)} • ${escapeHtml(plugin.plugin_type)}</p>
      </div>
      <span class="badge ${plugin.enabled ? "badge--success" : "badge--muted"}">${enabled}</span>
    </div>
    <p class="item__desc">${escapeHtml(plugin.description || "No description provided.")}</p>
    <div class="item__meta">
      <span>Author: ${escapeHtml(plugin.author || "Unknown")}</span>
      <span>Entry: ${escapeHtml(plugin.entry_point)}</span>
      <span>Updated: ${escapeHtml(formatTimestamp(plugin.updated_at))}</span>
    </div>
    <div class="item__actions">
      <button class="btn btn--ghost" type="button" data-action="prefill-exec">Run</button>
      <button class="btn btn--ghost" type="button" data-action="${toggleAction}">${toggleLabel}</button>
      <button class="btn btn--danger" type="button" data-action="uninstall-plugin">Uninstall</button>
    </div>
  `;

  return card;
}

function buildExecutionCard(execution: Execution) {
  const card = document.createElement("article");
  card.className = "item";
  card.dataset.executionId = execution.id;

  const status = escapeHtml(execution.status);
  const statusClass = `badge--status-${execution.status}`;
  const stdout = execution.stdout || "No stdout captured.";
  const stderr = execution.stderr || "No stderr captured.";
  const errorMessage = execution.error_message;
  const showStop = execution.status === "Running" || execution.status === "Pending";

  card.innerHTML = `
    <div class="item__header">
      <div>
        <h3 title="${escapeHtml(execution.id)}">Execution ${escapeHtml(shortId(execution.id))}</h3>
        <p class="meta">Plugin ${escapeHtml(execution.plugin_id)} • ${status}</p>
      </div>
      <span class="badge ${statusClass}">${status}</span>
    </div>
    <div class="item__grid">
      <div><span>PID</span><strong>${execution.pid ?? "--"}</strong></div>
      <div><span>Exit</span><strong>${execution.exit_code ?? "--"}</strong></div>
      <div><span>Started</span><strong>${formatTimestamp(execution.started_at)}</strong></div>
      <div><span>Finished</span><strong>${execution.finished_at ? formatTimestamp(execution.finished_at) : "--"}</strong></div>
    </div>
    ${errorMessage ? `<div class="item__alert">${escapeHtml(errorMessage)}</div>` : ""}
    <details class="log">
      <summary>Output</summary>
      <div class="log__grid">
        <div>
          <div class="meta">Stdout</div>
          <pre>${escapeHtml(stdout)}</pre>
        </div>
        <div>
          <div class="meta">Stderr</div>
          <pre>${escapeHtml(stderr)}</pre>
        </div>
      </div>
    </details>
    <div class="item__actions">
      ${
        showStop
          ? `<button class="btn btn--ghost" type="button" data-action="stop-execution">Stop</button>`
          : ""
      }
    </div>
  `;

  return card;
}

function escapeHtml(value: string) {
  const map: Record<string, string> = {
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  };
  return value.replace(/[&<>"']/g, (char) => map[char] || char);
}

function formatTimestamp(value: string) {
  if (!value) {
    return "--";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function shortId(id: string) {
  return id.length > 8 ? `${id.slice(0, 8)}...` : id;
}

function parseParams(raw: string) {
  const trimmed = raw.trim();
  if (!trimmed) {
    return {};
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch (error) {
    throw new Error("Params must be valid JSON.");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("Params must be a JSON object.");
  }
  return parsed as Record<string, unknown>;
}

async function handleInstall(event: SubmitEvent) {
  event.preventDefault();
  const data = new FormData(dom.installForm);
  const payload: InstallPluginRequest = {
    name: String(data.get("name") || ""),
    version: String(data.get("version") || ""),
    plugin_type: String(data.get("plugin_type") || ""),
    description: String(data.get("description") || ""),
    author: String(data.get("author") || ""),
    package_url: String(data.get("package_url") || ""),
    entry_point: String(data.get("entry_point") || ""),
    metadata: String(data.get("metadata") || "").trim() || null,
  };

  try {
    await api.installPlugin(payload);
    dom.installForm.reset();
    await loadPlugins();
    notify("Plugin installed.", "success");
  } catch (error) {
    const message = error instanceof Error ? error.message : "Failed to install plugin.";
    notify(message, "error");
  }
}

async function handleExecution(event: SubmitEvent) {
  event.preventDefault();
  const pluginId = dom.executionPluginId.value.trim();
  if (!pluginId) {
    notify("Plugin ID is required.", "error");
    return;
  }

  let params: Record<string, unknown> = {};
  try {
    params = parseParams(dom.executionParams.value);
  } catch (error) {
    const message = error instanceof Error ? error.message : "Invalid params.";
    notify(message, "error");
    return;
  }

  const payload: ExecutePluginRequest = {};
  if (Object.keys(params).length) {
    payload.params = params;
  }

  try {
    await api.executePlugin(pluginId, payload);
    dom.executionParams.value = "";
    await loadExecutions();
    notify("Execution started.", "success");
  } catch (error) {
    const message = error instanceof Error ? error.message : "Failed to execute plugin.";
    notify(message, "error");
  }
}

async function handlePluginAction(event: MouseEvent) {
  const target = event.target as HTMLElement | null;
  const button = target?.closest<HTMLButtonElement>("button[data-action]");
  if (!button) {
    return;
  }
  const card = button.closest<HTMLElement>("[data-plugin-id]");
  const pluginId = card?.dataset.pluginId;
  if (!pluginId) {
    return;
  }

  const action = button.dataset.action;
  if (action === "prefill-exec") {
    dom.executionPluginId.value = pluginId;
    document
      .querySelector("#execution-section")
      ?.scrollIntoView({ behavior: reducedMotion ? "auto" : "smooth" });
    dom.executionParams.focus();
    return;
  }

  if (action === "enable-plugin" || action === "disable-plugin") {
    const message = action === "enable-plugin" ? "Plugin enabled." : "Plugin disabled.";
    try {
      if (action === "enable-plugin") {
        await api.enablePlugin(pluginId);
      } else {
        await api.disablePlugin(pluginId);
      }
      await loadPlugins();
      notify(message, "success");
    } catch (error) {
      const detail = error instanceof Error ? error.message : "Plugin update failed.";
      notify(detail, "error");
    }
    return;
  }

  if (action === "uninstall-plugin") {
    const confirmed = window.confirm("Uninstall this plugin?");
    if (!confirmed) {
      return;
    }
    try {
      await api.uninstallPlugin(pluginId);
      await loadPlugins();
      notify("Plugin uninstalled.", "success");
    } catch (error) {
      const detail = error instanceof Error ? error.message : "Failed to uninstall plugin.";
      notify(detail, "error");
    }
  }
}

async function handleExecutionAction(event: MouseEvent) {
  const target = event.target as HTMLElement | null;
  const button = target?.closest<HTMLButtonElement>("button[data-action]");
  if (!button) {
    return;
  }
  const card = button.closest<HTMLElement>("[data-execution-id]");
  const executionId = card?.dataset.executionId;
  if (!executionId) {
    return;
  }

  const action = button.dataset.action;
  if (action === "stop-execution") {
    try {
      await api.stopExecution(executionId);
      await loadExecutions();
      notify("Execution stopped.", "success");
    } catch (error) {
      const detail = error instanceof Error ? error.message : "Failed to stop execution.";
      notify(detail, "error");
    }
  }
}

dom.connectForm.addEventListener("submit", (event) => {
  event.preventDefault();
  connect();
});

dom.refreshAll.addEventListener("click", () => {
  connect();
});

dom.scrollPlugins.addEventListener("click", () => {
  document
    .querySelector("#plugins-section")
    ?.scrollIntoView({ behavior: reducedMotion ? "auto" : "smooth" });
});

dom.refreshPlugins.addEventListener("click", async () => {
  try {
    await loadPlugins();
    notify("Plugins refreshed.", "success");
  } catch (error) {
    const message = error instanceof Error ? error.message : "Failed to refresh plugins.";
    notify(message, "error");
  }
});

dom.refreshExecutions.addEventListener("click", async () => {
  try {
    await loadExecutions();
    notify("Executions refreshed.", "success");
  } catch (error) {
    const message = error instanceof Error ? error.message : "Failed to refresh executions.";
    notify(message, "error");
  }
});

dom.installForm.addEventListener("submit", handleInstall);
dom.executionForm.addEventListener("submit", handleExecution);
dom.pluginList.addEventListener("click", handlePluginAction);
dom.executionList.addEventListener("click", handleExecutionAction);

dom.executionFilterForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  await loadExecutions();
});

dom.executionFilterClear.addEventListener("click", async () => {
  dom.executionFilterInput.value = "";
  await loadExecutions();
});

dom.baseUrlInput.value = state.baseUrl;
connect();
