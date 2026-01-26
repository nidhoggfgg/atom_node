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

type ExecutePluginRequest = {
  params?: Record<string, unknown>;
};

const dom = {
  baseUrlInput: document.querySelector<HTMLInputElement>("#base-url")!,
  connectForm: document.querySelector<HTMLFormElement>("#connect-form")!,
  connectionStatus: document.querySelector<HTMLButtonElement>("#connection-status")!,
  statusDot: document.querySelector<HTMLElement>("#status-dot")!,
  statusText: document.querySelector<HTMLElement>("#status-text")!,
  connectionState: document.querySelector<HTMLElement>("#connection-state")!,
  pluginSearch: document.querySelector<HTMLInputElement>("#plugin-search")!,
  pluginList: document.querySelector<HTMLElement>("#plugin-list")!,
  pluginCount: document.querySelector<HTMLElement>("#plugin-count")!,
  pluginDetail: document.querySelector<HTMLElement>("#plugin-detail")!,
  pluginModal: document.querySelector<HTMLElement>("#plugin-modal")!,
  pluginModalTitle: document.querySelector<HTMLElement>("#plugin-modal-title")!,
  pluginModalMeta: document.querySelector<HTMLElement>("#plugin-modal-meta")!,
  notice: document.querySelector<HTMLElement>("#notice")!,
  connectionModal: document.querySelector<HTMLElement>("#connection-modal")!,
  modalCloseButtons: document.querySelectorAll<HTMLElement>('[data-modal="close"]'),
  modalBackdrops: document.querySelectorAll<HTMLElement>(".modal__backdrop"),
};

const state = {
  baseUrl: localStorage.getItem("atom_node_base_url") || "http://localhost:3000",
  plugins: [] as Plugin[],
  executions: [] as Execution[],
  connected: false,
  selectedPlugin: null as Plugin | null,
  filterText: "",
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
  async enablePlugin(id: string): Promise<void> {
    await request<void>(`/api/plugins/${id}/enable`, { method: "PUT" });
  },
  async disablePlugin(id: string): Promise<void> {
    await request<void>(`/api/plugins/${id}/disable`, { method: "PUT" });
  },
  async uninstallPlugin(id: string): Promise<void> {
    await request<void>(`/api/plugins/${id}`, { method: "DELETE" });
  },
  async updatePlugin(id: string): Promise<Plugin> {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const response = await request<Plugin>(`/api/plugins/${id}`);
    return response;
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

function updateConnectionState(connected: boolean) {
  state.connected = connected;
  if (connected) {
    dom.connectionStatus.classList.add("connected");
    dom.statusText.textContent = "已连接";
    dom.connectionState.textContent = "在线";
  } else {
    dom.connectionStatus.classList.remove("connected");
    dom.statusText.textContent = "未连接";
    dom.connectionState.textContent = "离线";
  }
}

function openModal(modal: HTMLElement) {
  modal.classList.add("active");
}

function closeModal(modal: HTMLElement) {
  modal.classList.remove("active");
  if (modal === dom.pluginModal) {
    state.selectedPlugin = null;
    renderPluginList();
  }
}

async function connect() {
  const baseUrl = normalizeBaseUrl(dom.baseUrlInput.value.trim());
  if (!baseUrl) {
    notify("请输入服务地址。", "error");
    return;
  }
  state.baseUrl = baseUrl;
  localStorage.setItem("atom_node_base_url", baseUrl);

  const submitBtn = dom.connectForm.querySelector('button[type="submit"]') as HTMLButtonElement;
  submitBtn.disabled = true;
  submitBtn.textContent = "正在连接...";

  try {
    const health = await api.health();
    updateConnectionState(true);
    closeModal(dom.connectionModal);
    await Promise.all([loadPlugins()]);
    notify("已连接到 Atom Node 服务。", "success");
  } catch (error) {
    const message = error instanceof Error ? error.message : "连接失败。";
    updateConnectionState(false);
    notify(message, "error");
  } finally {
    submitBtn.disabled = false;
    submitBtn.textContent = "连接";
  }
}

async function loadPlugins() {
  try {
    const response = await api.listPlugins();
    state.plugins = response.data || [];
    renderPluginList();
  } catch (error) {
    const message = error instanceof Error ? error.message : "加载插件失败。";
    notify(message, "error");
  }
}

async function loadExecutions() {
  if (!state.selectedPlugin) return;

  try {
    const response = await api.listExecutions(state.selectedPlugin.id);
    state.executions = response.data || [];
    renderExecutions();
  } catch (error) {
    const message = error instanceof Error ? error.message : "加载执行记录失败。";
    notify(message, "error");
  }
}

function getFilteredPlugins(): Plugin[] {
  if (!state.filterText) return state.plugins;
  const search = state.filterText.toLowerCase();
  return state.plugins.filter(
    (p) =>
      p.name.toLowerCase().includes(search) ||
      p.description?.toLowerCase().includes(search) ||
      p.author?.toLowerCase().includes(search)
  );
}

function renderPluginList() {
  const plugins = getFilteredPlugins();

  dom.pluginList.innerHTML = "";

  const total = state.plugins.length;
  const shown = plugins.length;
  if (state.filterText) {
    dom.pluginCount.textContent = `${shown} / ${total}`;
  } else {
    dom.pluginCount.textContent = `${total}`;
  }

  if (plugins.length === 0) {
    dom.pluginList.innerHTML = `
      <div class="empty-state empty-state--wide">
        <p>${state.filterText ? "没有匹配的插件。" : "暂未安装插件。"}</p>
      </div>
    `;
    return;
  }

  const fragment = document.createDocumentFragment();
  plugins.forEach((plugin) => fragment.append(buildPluginCard(plugin)));
  dom.pluginList.append(fragment);
}

function renderExecutions() {
  const executionsSection = document.getElementById("executions-section");
  if (!executionsSection) return;

  const list = executionsSection.querySelector(".executions-list")!;
  list.innerHTML = "";

  if (state.executions.length === 0) {
    list.innerHTML = `<div class="empty-state"><p>暂无执行记录。</p></div>`;
    return;
  }

  const fragment = document.createDocumentFragment();
  state.executions.forEach((execution) => fragment.append(buildExecutionCard(execution)));
  list.append(fragment);
}

function renderPluginDetail(plugin: Plugin) {
  const enabledBadge = plugin.enabled
    ? `<span class="badge badge--success">已启用</span>`
    : `<span class="badge badge--muted">已停用</span>`;

  const toggleAction = plugin.enabled ? "disable" : "enable";
  const toggleLabel = plugin.enabled ? "禁用" : "启用";

  dom.pluginModalTitle.textContent = plugin.name;
  dom.pluginModalMeta.innerHTML = `
    <span>版本: ${escapeHtml(plugin.version)}</span>
    <span>作者: ${escapeHtml(plugin.author || "未知")}</span>
    ${enabledBadge}
  `;

  dom.pluginDetail.innerHTML = `
    <div class="plugin-detail__section">
      <h3 class="plugin-detail__section-title">概览</h3>
      <p class="plugin-detail__description">${escapeHtml(plugin.description || "暂无描述。")}</p>
      <div class="plugin-detail__meta">
        <span>更新于: ${formatTimestamp(plugin.updated_at)}</span>
      </div>
    </div>

    <div class="plugin-detail__section">
      <h3 class="plugin-detail__section-title">运行插件</h3>
      <form id="execution-form" class="execution-form">
        <div id="execution-dynamic-fields"></div>
        <div class="form__actions">
          <button class="btn btn--primary" type="submit" id="btn-execute">运行插件</button>
        </div>
      </form>
    </div>

    <div class="plugin-detail__section" id="executions-section">
      <h3 class="plugin-detail__section-title">最近执行</h3>
      <div class="executions-list"></div>
    </div>

    <div class="plugin-detail__section">
      <h3 class="plugin-detail__section-title">操作</h3>
      <div class="form__actions">
        <button class="btn btn--secondary" type="button" data-action="${toggleAction}">${toggleLabel}</button>
        <button class="btn btn--danger" type="button" data-action="uninstall">卸载</button>
      </div>
    </div>
  `;

  renderExecutionForm(plugin);

  const executionForm = dom.pluginDetail.querySelector<HTMLFormElement>("#execution-form")!;
  executionForm.addEventListener("submit", handleExecution);

  const executeBtn = document.getElementById("btn-execute") as HTMLButtonElement;
  if (executeBtn && !plugin.enabled) {
    executeBtn.disabled = true;
    executeBtn.title = "需先启用插件才能运行";
  }

  const actionButtons = dom.pluginDetail.querySelectorAll<HTMLButtonElement>('button[data-action]');
  actionButtons.forEach((btn) => {
    btn.addEventListener("click", () => handlePluginAction(btn.dataset.action!, plugin.id));
  });
}

function renderExecutionForm(plugin: Plugin) {
  const container = document.getElementById("execution-dynamic-fields");
  if (!container) return;

  container.innerHTML = "";

  if (!plugin.parameters || plugin.parameters.length === 0) {
    container.innerHTML = `<p class="muted">该插件没有参数。</p>`;
    return;
  }

  const fragment = document.createDocumentFragment();
  plugin.parameters.forEach((param) => {
    fragment.appendChild(buildParameterField(param));
  });
  container.appendChild(fragment);
}

function buildParameterField(param: PluginParameter): HTMLElement {
  const label = document.createElement("label");
  label.className = "field";

  const labelText = document.createElement("span");
  labelText.textContent = param.description || param.name;
  label.appendChild(labelText);

  const defaultVal = param.default !== undefined ? String(param.default) : "";

  switch (param.param_type) {
    case "string":
      const stringInput = document.createElement("input");
      stringInput.type = "text";
      stringInput.name = param.name;
      stringInput.placeholder = `输入 ${param.name}`;
      if (defaultVal) stringInput.value = defaultVal;
      label.appendChild(stringInput);
      break;

    case "number":
      const numberInput = document.createElement("input");
      numberInput.type = "number";
      numberInput.name = param.name;
      numberInput.placeholder = `输入 ${param.name}`;
      numberInput.step = "any";
      if (defaultVal) numberInput.value = defaultVal;
      label.appendChild(numberInput);
      break;

    case "integer":
      const integerInput = document.createElement("input");
      integerInput.type = "number";
      integerInput.name = param.name;
      integerInput.placeholder = `输入 ${param.name}`;
      integerInput.step = "1";
      if (defaultVal) integerInput.value = defaultVal;
      label.appendChild(integerInput);
      break;

    case "boolean":
      const booleanWrapper = document.createElement("div");
      booleanWrapper.className = "checkbox-wrapper";

      const booleanInput = document.createElement("input");
      booleanInput.type = "checkbox";
      booleanInput.name = param.name;
      booleanInput.id = `param-${param.name}`;
      if (defaultVal === "true") booleanInput.checked = true;

      const booleanLabel = document.createElement("label");
      booleanLabel.htmlFor = `param-${param.name}`;
      booleanLabel.textContent = param.description || param.name;

      booleanWrapper.appendChild(booleanInput);
      booleanWrapper.appendChild(booleanLabel);

      label.textContent = "";
      label.appendChild(booleanWrapper);
      break;

    case "json":
      const jsonTextarea = document.createElement("textarea");
      jsonTextarea.name = param.name;
      jsonTextarea.placeholder = `输入 ${param.name} 的 JSON 值`;
      jsonTextarea.rows = 4;
      if (defaultVal) jsonTextarea.value = defaultVal;
      label.appendChild(jsonTextarea);
      break;
  }

  return label;
}

function buildPluginCard(plugin: Plugin) {
  const card = document.createElement("div");
  card.className = `plugin-card ${state.selectedPlugin?.id === plugin.id ? "active" : ""}`;
  card.dataset.pluginId = plugin.id;

  const enabledBadge = plugin.enabled
    ? `<span class="badge badge--success">已启用</span>`
    : `<span class="badge badge--muted">已停用</span>`;

  card.innerHTML = `
    <div class="plugin-card__header">
      <div>
        <h3 class="plugin-card__title">${escapeHtml(plugin.name)}</h3>
        <div class="plugin-card__meta">v${escapeHtml(plugin.version)} • ${escapeHtml(plugin.author || "未知")}</div>
      </div>
      ${enabledBadge}
    </div>
    <p class="plugin-card__desc">${escapeHtml(plugin.description || "暂无描述。")}</p>
    <div class="plugin-card__footer">
      <span>更新于 ${formatTimestamp(plugin.updated_at)}</span>
      <button class="btn btn--primary btn--small" type="button" data-action="open">运行</button>
    </div>
  `;

  const openBtn = card.querySelector<HTMLButtonElement>('button[data-action="open"]');
  if (openBtn) {
    openBtn.addEventListener("click", (event) => {
      event.stopPropagation();
      selectPlugin(plugin.id);
    });
  }

  card.addEventListener("click", () => selectPlugin(plugin.id));

  return card;
}

function buildExecutionCard(execution: Execution) {
  const card = document.createElement("div");
  card.className = "execution-card";

  const statusClass = `execution-card__status--${execution.status.toLowerCase()}`;

  const showStop = execution.status === "Running" || execution.status === "Pending";

  card.innerHTML = `
    <div class="execution-card__header">
      <div class="execution-card__meta-line">开始时间：${formatDateTime(execution.started_at)}</div>
      <span class="execution-card__status ${statusClass}">${escapeHtml(execution.status)}</span>
    </div>
    <div class="execution-card__meta">
      <div><span>用时</span><strong>${calculateDuration(execution.started_at, execution.finished_at)}</strong></div>
      <div><span>结束时间</span><strong>${execution.finished_at ? formatDateTime(execution.finished_at) : "--"}</strong></div>
    </div>
    ${execution.error_message ? `<p style="color: #9a2a1d; font-size: 0.85rem; margin-top: 8px;">${escapeHtml(execution.error_message)}</p>` : ""}
    <div class="execution-card__output">
      <summary>输出</summary>
      <pre>${escapeHtml(execution.stdout || "无输出。")}</pre>
    </div>
  `;

  if (showStop) {
    const stopBtn = document.createElement("button");
    stopBtn.className = "btn btn--danger";
    stopBtn.textContent = "停止";
    stopBtn.style.marginTop = "12px";
    stopBtn.addEventListener("click", () => stopExecution(execution.id));
    card.appendChild(stopBtn);
  }

  return card;
}

function selectPlugin(pluginId: string) {
  const plugin = state.plugins.find((p) => p.id === pluginId);
  if (!plugin) return;

  state.selectedPlugin = plugin;
  renderPluginList();
  renderPluginDetail(plugin);
  openModal(dom.pluginModal);
  loadExecutions();
}

async function handleExecution(event: SubmitEvent) {
  event.preventDefault();
  if (!state.selectedPlugin) {
    notify("未选择插件。", "error");
    return;
  }

  if (!state.selectedPlugin.enabled) {
    notify("需先启用插件才能运行。", "error");
    return;
  }

  const executionForm = dom.pluginDetail.querySelector<HTMLFormElement>("#execution-form");
  if (!executionForm) return;

  const formData = new FormData(executionForm);
  const params: Record<string, unknown> = {};

  if (state.selectedPlugin.parameters && state.selectedPlugin.parameters.length > 0) {
    state.selectedPlugin.parameters.forEach((param) => {
      const value = formData.get(param.name);
      if (value !== null && value !== "") {
        switch (param.param_type) {
          case "string":
            params[param.name] = String(value);
            break;
          case "number":
            params[param.name] = Number(value);
            break;
          case "integer":
            params[param.name] = Number.parseInt(String(value), 10);
            break;
          case "boolean":
            params[param.name] = (formData.get(`${param.name}-checkbox`) as string) === "on";
            break;
          case "json":
            try {
              params[param.name] = JSON.parse(String(value));
            } catch {
              params[param.name] = String(value);
            }
            break;
        }
      }
    });
  }

  const payload: ExecutePluginRequest = {};
  if (Object.keys(params).length) {
    payload.params = params;
  }

  const executeBtn = document.getElementById("btn-execute") as HTMLButtonElement;
  if (executeBtn) {
    executeBtn.disabled = true;
    executeBtn.textContent = "正在运行...";
  }

  try {
    await api.executePlugin(state.selectedPlugin.id, payload);
    await loadExecutions();
    notify("插件已开始执行。", "success");
  } catch (error) {
    const message = error instanceof Error ? error.message : "执行插件失败。";
    notify(message, "error");
  } finally {
    if (executeBtn) {
      executeBtn.disabled = false;
      executeBtn.textContent = "运行插件";
    }
  }
}

async function handlePluginAction(action: string, pluginId: string) {
  if (action === "update") {
    const confirmed = window.confirm("是否更新该插件？");
    if (!confirmed) return;

    try {
      await api.updatePlugin(pluginId);
      await loadPlugins();
      const updatedPlugin = state.plugins.find((plugin) => plugin.id === pluginId) || null;
      state.selectedPlugin = updatedPlugin;
      if (updatedPlugin) {
        renderPluginDetail(updatedPlugin);
      }
      notify("插件更新成功。", "success");
    } catch (error) {
      const detail = error instanceof Error ? error.message : "更新插件失败。";
      notify(detail, "error");
    }
    return;
  }

  if (action === "enable" || action === "disable") {
    const message = action === "enable" ? "插件已启用。" : "插件已停用。";
    try {
      if (action === "enable") {
        await api.enablePlugin(pluginId);
      } else {
        await api.disablePlugin(pluginId);
      }
      await loadPlugins();
      const updatedPlugin = state.plugins.find((plugin) => plugin.id === pluginId) || null;
      state.selectedPlugin = updatedPlugin;
      if (updatedPlugin) {
        renderPluginDetail(updatedPlugin);
      }
      notify(message, "success");
    } catch (error) {
      const detail = error instanceof Error ? error.message : "插件更新失败。";
      notify(detail, "error");
    }
    return;
  }

  if (action === "uninstall") {
    const confirmed = window.confirm("是否卸载该插件？");
    if (!confirmed) return;

    try {
      await api.uninstallPlugin(pluginId);
      state.selectedPlugin = null;
      await loadPlugins();
      closeModal(dom.pluginModal);
      notify("插件已卸载。", "success");
    } catch (error) {
      const detail = error instanceof Error ? error.message : "卸载插件失败。";
      notify(detail, "error");
    }
  }
}

async function stopExecution(executionId: string) {
  try {
    await api.stopExecution(executionId);
    await loadExecutions();
    notify("执行已停止。", "success");
  } catch (error) {
    const detail = error instanceof Error ? error.message : "停止执行失败。";
    notify(detail, "error");
  }
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
  if (!value) return "--";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString();
}

function formatDateTime(value: string) {
  if (!value) return "--";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function calculateDuration(startedAt: string, finishedAt: string | null): string {
  const start = new Date(startedAt).getTime();
  const end = finishedAt ? new Date(finishedAt).getTime() : Date.now();
  const durationMs = Math.max(0, end - start);
  const totalSeconds = durationMs / 1000;
  const minutes = Math.floor(totalSeconds / 60);
  const hours = Math.floor(minutes / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}小时 ${minutes % 60}分 ${seconds.toFixed(1)}秒`;
  }
  if (minutes > 0) {
    return `${minutes}分 ${seconds.toFixed(1)}秒`;
  }
  if (totalSeconds >= 1) {
    return `${totalSeconds.toFixed(1)}秒`;
  }
  return `${Math.round(durationMs)}毫秒`;
}

dom.connectionStatus.addEventListener("click", () => {
  dom.baseUrlInput.value = state.baseUrl;
  openModal(dom.connectionModal);
});

dom.connectForm.addEventListener("submit", (event) => {
  event.preventDefault();
  connect();
});

dom.pluginSearch.addEventListener("input", (event) => {
  state.filterText = (event.target as HTMLInputElement).value.trim();
  renderPluginList();
});

dom.modalCloseButtons.forEach((btn) => {
  btn.addEventListener("click", () => {
    const modal = btn.closest(".modal") as HTMLElement;
    closeModal(modal);
  });
});

dom.modalBackdrops.forEach((backdrop) => {
  backdrop.addEventListener("click", () => {
    const modal = backdrop.closest(".modal") as HTMLElement;
    closeModal(modal);
  });
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    document.querySelectorAll<HTMLElement>(".modal.active").forEach(closeModal);
  }
});

dom.baseUrlInput.value = state.baseUrl;

if (state.baseUrl && state.baseUrl !== "http://localhost:3000") {
  connect();
} else {
  renderPluginList();
}
