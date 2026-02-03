type PluginParamType =
  | "string"
  | "number"
  | "integer"
  | "boolean"
  | "json"
  | "date"
  | "select"
  | "multi_select"
  | "file"
  | "directory"
  | "textarea";

type PluginParameterChoice = {
  label: string;
  value: unknown;
};

type PluginParameterValidation = {
  min?: number;
  max?: number;
};

type PluginParameter = {
  name: string;
  type: PluginParamType;
  description?: string | null;
  default?: unknown;
  choices?: unknown[] | null;
  label?: string | null;
  placeholder?: string | null;
  group?: string | null;
  format?: string | null;
  accept?: string[] | null;
  validation?: PluginParameterValidation | null;
};

type PluginParameterGroup = {
  id: string;
  label: string;
};

type Plugin = {
  id: string;
  plugin_id?: string;
  name: string;
  version: string;
  min_anthill_version?: string | null;
  plugin_type: string;
  description: string;
  author: string;
  entry_point: string;
  enabled: boolean;
  created_at: string | number;
  updated_at: string | number;
  parameters?: PluginParameter[] | null;
  groups?: PluginParameterGroup[] | null;
  metadata?: Record<string, unknown> | null;
};

type PluginPayload = Omit<Plugin, "id"> & { id?: string; plugin_id?: string };

type Execution = {
  id: string;
  plugin_id: string;
  status: string;
  pid: number | null;
  exit_code: number | null;
  stdout: string | null;
  stderr: string | null;
  started_at: string | number;
  finished_at: string | number | null;
  error_message: string | null;
};

type HealthResponse = {
  status: string;
  service: string;
  version: string;
};

type PluginsListResponse = { data: PluginPayload[] };
type ExecutionsListResponse = { data: Execution[] };

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
  openInstallButton: document.querySelector<HTMLButtonElement>("#open-install-modal")!,
  installModal: document.querySelector<HTMLElement>("#install-modal")!,
  installForm: document.querySelector<HTMLFormElement>("#install-form")!,
  installPathInput: document.querySelector<HTMLInputElement>("#install-path")!,
  installFileInput: document.querySelector<HTMLInputElement>("#install-file")!,
  notice: document.querySelector<HTMLElement>("#notice")!,
  connectionModal: document.querySelector<HTMLElement>("#connection-modal")!,
  modalCloseButtons: document.querySelectorAll<HTMLElement>('[data-modal="close"]'),
  modalBackdrops: document.querySelectorAll<HTMLElement>(".modal__backdrop"),
};

const state = {
  baseUrl: localStorage.getItem("anthill_base_url") || "http://localhost:6701",
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
  async installPlugin(packageUrl: string): Promise<PluginPayload> {
    return request<PluginPayload>("/api/plugins", {
      method: "POST",
      body: JSON.stringify({ package_url: packageUrl }),
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
  async updatePlugin(id: string): Promise<PluginPayload> {
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

function resolveFilePath(file: File): string | null {
  const fileWithPath = file as File & { path?: string; webkitRelativePath?: string };
  if (fileWithPath.path && fileWithPath.path.trim()) {
    return fileWithPath.path;
  }
  if (fileWithPath.webkitRelativePath && fileWithPath.webkitRelativePath.trim()) {
    return fileWithPath.webkitRelativePath;
  }
  return null;
}

function normalizePlugin(plugin: PluginPayload): Plugin {
  const resolvedId = plugin.id || plugin.plugin_id || plugin.name;
  return { ...plugin, id: resolvedId };
}

function normalizeChoices(choices?: unknown[] | null): PluginParameterChoice[] {
  if (!choices || choices.length === 0) return [];
  return choices.map((choice) => {
    if (choice && typeof choice === "object" && "value" in (choice as Record<string, unknown>)) {
      const record = choice as { label?: unknown; value?: unknown };
      return {
        label: record.label !== undefined ? String(record.label) : String(record.value ?? ""),
        value: record.value,
      };
    }
    return { label: String(choice), value: choice };
  });
}

function serializeChoiceValue(value: unknown): string {
  if (value === undefined) return "";
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function parseChoiceValue(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

function formatDefaultValue(param: PluginParameter): string | undefined {
  if (param.default === undefined || param.default === null) return undefined;
  if (param.type === "json") {
    if (typeof param.default === "string") return param.default;
    try {
      return JSON.stringify(param.default, null, 2);
    } catch {
      return String(param.default);
    }
  }
  return typeof param.default === "string" ? param.default : String(param.default);
}

function getParamLabel(param: PluginParameter): string {
  return (param.label || param.name).trim();
}

function getParamHint(param: PluginParameter, labelText: string): string | null {
  const hints: string[] = [];
  if (param.description && param.description !== labelText) {
    hints.push(param.description);
  }
  if (param.format) {
    hints.push(`格式: ${param.format}`);
  }
  if (param.accept && param.accept.length > 0) {
    hints.push(`允许: ${param.accept.join(", ")}`);
  }
  if (param.validation && (param.validation.min !== undefined || param.validation.max !== undefined)) {
    const minText = param.validation.min !== undefined ? `最小 ${param.validation.min}` : "";
    const maxText = param.validation.max !== undefined ? `最大 ${param.validation.max}` : "";
    hints.push([minText, maxText].filter(Boolean).join(" / "));
  }
  return hints.length > 0 ? hints.join(" · ") : null;
}

function groupParameters(
  parameters: PluginParameter[],
  groups?: PluginParameterGroup[] | null
): Array<{ id: string; label: string; items: PluginParameter[] }> {
  const grouped = new Map<string, PluginParameter[]>();
  parameters.forEach((param) => {
    const groupId = (param.group || "default").trim();
    if (!grouped.has(groupId)) {
      grouped.set(groupId, []);
    }
    grouped.get(groupId)!.push(param);
  });

  const result: Array<{ id: string; label: string; items: PluginParameter[] }> = [];
  const used = new Set<string>();

  if (groups && groups.length > 0) {
    groups.forEach((group) => {
      const items = grouped.get(group.id);
      if (items && items.length > 0) {
        result.push({ id: group.id, label: group.label, items });
        used.add(group.id);
      }
    });
  }

  grouped.forEach((items, id) => {
    if (used.has(id)) return;
    const label = id === "default" ? "默认参数" : id;
    result.push({ id, label, items });
  });

  return result;
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
  localStorage.setItem("anthill_base_url", baseUrl);

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
    state.plugins = (response.data || []).map(normalizePlugin);
    renderPluginList();
  } catch (error) {
    const message = error instanceof Error ? error.message : "加载插件失败。";
    notify(message, "error");
  }
}

async function handleInstall(event: SubmitEvent) {
  event.preventDefault();

  if (!state.connected) {
    notify("请先连接到服务。", "error");
    closeModal(dom.installModal);
    openModal(dom.connectionModal);
    return;
  }

  const packageUrl = dom.installPathInput.value.trim();
  if (!packageUrl) {
    notify("请输入插件包路径。", "error");
    return;
  }

  const submitBtn = dom.installForm.querySelector<HTMLButtonElement>("#btn-install");
  if (submitBtn) {
    submitBtn.disabled = true;
    submitBtn.textContent = "正在安装...";
  }

  try {
    await api.installPlugin(packageUrl);
    await loadPlugins();
    dom.installForm.reset();
    closeModal(dom.installModal);
    notify("插件安装成功。", "success");
  } catch (error) {
    const detail = error instanceof Error ? error.message : "安装插件失败。";
    notify(detail, "error");
  } finally {
    if (submitBtn) {
      submitBtn.disabled = false;
      submitBtn.textContent = "安装";
    }
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
      p.plugin_id?.toLowerCase().includes(search) ||
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

  const minAtomNodeVersion = plugin.min_anthill_version
    ? escapeHtml(plugin.min_anthill_version)
    : "无";

  const toggleAction = plugin.enabled ? "disable" : "enable";
  const toggleLabel = plugin.enabled ? "禁用" : "启用";

  dom.pluginModalTitle.textContent = plugin.name;
  dom.pluginModalMeta.innerHTML = `
    <span>版本: ${escapeHtml(plugin.version)}</span>
    <span>最低 Atom Node 版本: ${minAtomNodeVersion}</span>
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
  const parameterGroups = groupParameters(plugin.parameters, plugin.groups);
  parameterGroups.forEach((group) => {
    const groupWrapper = document.createElement("div");
    groupWrapper.className = "parameter-group";
    if (parameterGroups.length > 1 || group.id !== "default") {
      const groupTitle = document.createElement("h4");
      groupTitle.className = "parameter-group__title";
      groupTitle.textContent = group.label;
      groupWrapper.appendChild(groupTitle);
    }
    group.items.forEach((param) => {
      groupWrapper.appendChild(buildParameterField(param));
    });
    fragment.appendChild(groupWrapper);
  });
  container.appendChild(fragment);
}

function buildParameterField(param: PluginParameter): HTMLElement {
  const isBoolean = param.type === "boolean";
  const field = document.createElement(isBoolean ? "div" : "label");
  field.className = "field";

  const labelText = getParamLabel(param);
  const hintText = getParamHint(param, labelText);

  if (!isBoolean) {
    const labelSpan = document.createElement("span");
    labelSpan.textContent = labelText;
    field.appendChild(labelSpan);
  }

  if (hintText) {
    const hint = document.createElement("p");
    hint.className = "field__hint";
    hint.textContent = hintText;
    field.appendChild(hint);
  }

  const choices = normalizeChoices(param.choices || undefined);
  const hasChoices = choices.length > 0;
  const isMultiSelect = param.type === "multi_select";
  const isSelectType = param.type === "select" || (param.type === "string" && hasChoices);

  if (isMultiSelect) {
    const multiWrapper = document.createElement("div");
    multiWrapper.className = "multi-select";

    const defaultValues = new Set<string>();
    if (Array.isArray(param.default)) {
      param.default.forEach((value) => defaultValues.add(serializeChoiceValue(value)));
    }

    if (choices.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = "暂无可选项。";
      multiWrapper.appendChild(empty);
    } else {
      choices.forEach((choice, index) => {
        const option = document.createElement("label");
        option.className = "multi-select__option";

        const checkbox = document.createElement("input");
        checkbox.type = "checkbox";
        checkbox.name = param.name;
        checkbox.value = serializeChoiceValue(choice.value);
        checkbox.id = `param-${param.name}-${index}`;
        if (defaultValues.has(checkbox.value)) {
          checkbox.checked = true;
        }

        const text = document.createElement("span");
        text.textContent = choice.label;

        option.appendChild(checkbox);
        option.appendChild(text);
        multiWrapper.appendChild(option);
      });
    }

    field.appendChild(multiWrapper);
    return field;
  }

  if (isSelectType) {
    const select = document.createElement("select");
    select.name = param.name;

    const defaultValues = new Set<string>();
    if (Array.isArray(param.default)) {
      param.default.forEach((value) => defaultValues.add(serializeChoiceValue(value)));
    } else if (param.default !== undefined && param.default !== null) {
      defaultValues.add(serializeChoiceValue(param.default));
    }

    if (defaultValues.size === 0) {
      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = param.placeholder || "请选择";
      placeholder.disabled = true;
      placeholder.selected = true;
      select.appendChild(placeholder);
    }

    choices.forEach((choice) => {
      const option = document.createElement("option");
      const serialized = serializeChoiceValue(choice.value);
      option.value = serialized;
      option.textContent = choice.label;
      if (defaultValues.has(serialized)) {
        option.selected = true;
      }
      select.appendChild(option);
    });

    field.appendChild(select);
    return field;
  }

  const defaultVal = formatDefaultValue(param);
  const placeholder = param.placeholder || `输入 ${labelText}`;

  switch (param.type) {
    case "string": {
      const stringInput = document.createElement("input");
      stringInput.type = "text";
      stringInput.name = param.name;
      stringInput.placeholder = placeholder;
      if (defaultVal) stringInput.value = defaultVal;
      field.appendChild(stringInput);
      break;
    }

    case "date": {
      const dateInput = document.createElement("input");
      dateInput.type = "date";
      dateInput.name = param.name;
      dateInput.placeholder = param.placeholder || param.format || "";
      if (defaultVal) dateInput.value = defaultVal;
      field.appendChild(dateInput);
      break;
    }

    case "number": {
      const numberInput = document.createElement("input");
      numberInput.type = "number";
      numberInput.name = param.name;
      numberInput.placeholder = placeholder;
      numberInput.step = "any";
      if (param.validation?.min !== undefined) {
        numberInput.min = String(param.validation.min);
      }
      if (param.validation?.max !== undefined) {
        numberInput.max = String(param.validation.max);
      }
      if (defaultVal) numberInput.value = defaultVal;
      field.appendChild(numberInput);
      break;
    }

    case "integer": {
      const integerInput = document.createElement("input");
      integerInput.type = "number";
      integerInput.name = param.name;
      integerInput.placeholder = placeholder;
      integerInput.step = "1";
      if (param.validation?.min !== undefined) {
        integerInput.min = String(param.validation.min);
      }
      if (param.validation?.max !== undefined) {
        integerInput.max = String(param.validation.max);
      }
      if (defaultVal) integerInput.value = defaultVal;
      field.appendChild(integerInput);
      break;
    }

    case "boolean": {
      const booleanWrapper = document.createElement("div");
      booleanWrapper.className = "checkbox-wrapper";

      const booleanInput = document.createElement("input");
      booleanInput.type = "checkbox";
      booleanInput.name = param.name;
      booleanInput.id = `param-${param.name}`;
      if (param.default === true || param.default === "true") {
        booleanInput.checked = true;
      }

      const booleanLabel = document.createElement("label");
      booleanLabel.htmlFor = `param-${param.name}`;
      booleanLabel.textContent = labelText;

      booleanWrapper.appendChild(booleanInput);
      booleanWrapper.appendChild(booleanLabel);
      field.appendChild(booleanWrapper);
      break;
    }

    case "textarea": {
      const textarea = document.createElement("textarea");
      textarea.name = param.name;
      textarea.placeholder = placeholder;
      textarea.rows = 4;
      if (defaultVal) textarea.value = defaultVal;
      field.appendChild(textarea);
      break;
    }

    case "file":
    case "directory": {
      const wrapper = document.createElement("div");
      wrapper.className = "path-field";

      const pathInput = document.createElement("input");
      pathInput.type = "text";
      pathInput.name = param.name;
      pathInput.placeholder =
        param.placeholder || (param.type === "directory" ? "输入目录路径或选择目录" : "输入文件路径或选择文件");
      if (defaultVal) pathInput.value = defaultVal;

      const picker = document.createElement("input");
      picker.type = "file";
      picker.className = "path-field__picker";
      if (param.type === "directory") {
        (picker as HTMLInputElement).setAttribute("webkitdirectory", "");
      }
      if (param.accept && param.accept.length > 0) {
        picker.accept = param.accept.join(",");
      }

      picker.addEventListener("change", () => {
        const file = picker.files?.[0];
        if (!file) return;
        const fallback = (file as File & { webkitRelativePath?: string }).webkitRelativePath;
        pathInput.value = fallback && fallback.includes("/") ? fallback.split("/")[0] : file.name;
      });

      wrapper.appendChild(pathInput);
      wrapper.appendChild(picker);
      field.appendChild(wrapper);
      break;
    }

    case "json":
    default: {
      const jsonTextarea = document.createElement("textarea");
      jsonTextarea.name = param.name;
      jsonTextarea.placeholder = param.placeholder || `输入 ${labelText} 的 JSON 值`;
      jsonTextarea.rows = 4;
      if (defaultVal) jsonTextarea.value = defaultVal;
      field.appendChild(jsonTextarea);
      break;
    }
  }

  return field;
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
      if (param.type === "boolean") {
        const input = executionForm.querySelector<HTMLInputElement>(`[name="${param.name}"]`);
        if (input) {
          const checked = input.checked;
          if (checked || param.default === true || param.default === "true") {
            params[param.name] = checked;
          }
        }
        return;
      }

      if (param.type === "multi_select") {
        const values = formData
          .getAll(param.name)
          .map((value) => String(value))
          .filter((value) => value !== "");
        if (values.length > 0) {
          params[param.name] = values.map((value) => parseChoiceValue(value));
        }
        return;
      }

      const value = formData.get(param.name);
      if (value === null || value === "") {
        return;
      }

      const hasChoices = Array.isArray(param.choices) && param.choices.length > 0;
      if (param.type === "select" || (param.type === "string" && hasChoices)) {
        params[param.name] = parseChoiceValue(String(value));
        return;
      }

      switch (param.type) {
        case "string":
        case "date":
        case "textarea":
        case "file":
        case "directory":
          params[param.name] = String(value);
          break;
        case "number":
          params[param.name] = Number(value);
          break;
        case "integer":
          params[param.name] = Number.parseInt(String(value), 10);
          break;
        case "json":
        default:
          try {
            params[param.name] = JSON.parse(String(value));
          } catch {
            params[param.name] = String(value);
          }
          break;
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

function toTimestampMs(value: string | number | null | undefined): number | null {
  if (value === null || value === undefined) return null;
  if (typeof value === "number") {
    return value < 1e12 ? value * 1000 : value;
  }
  const trimmed = value.trim();
  if (!trimmed) return null;
  const numeric = Number(trimmed);
  if (!Number.isNaN(numeric)) {
    return numeric < 1e12 ? numeric * 1000 : numeric;
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.getTime();
}

function formatTimestamp(value: string | number | null | undefined) {
  const timestamp = toTimestampMs(value ?? null);
  if (!timestamp) return "--";
  const date = new Date(timestamp);
  return date.toLocaleDateString();
}

function formatDateTime(value: string | number | null | undefined) {
  const timestamp = toTimestampMs(value ?? null);
  if (!timestamp) return "--";
  const date = new Date(timestamp);
  return date.toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function calculateDuration(
  startedAt: string | number | null | undefined,
  finishedAt: string | number | null
): string {
  const start = toTimestampMs(startedAt ?? null) ?? Date.now();
  const end = finishedAt ? toTimestampMs(finishedAt) ?? Date.now() : Date.now();
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

dom.openInstallButton.addEventListener("click", () => {
  openModal(dom.installModal);
});

dom.installForm.addEventListener("submit", handleInstall);

dom.installFileInput.addEventListener("change", () => {
  const file = dom.installFileInput.files?.[0];
  if (!file) return;
  const resolvedPath = resolveFilePath(file);
  if (resolvedPath) {
    dom.installPathInput.value = resolvedPath;
    return;
  }
  dom.installPathInput.value = file.name;
  notify("无法读取完整路径，请手动填写服务器路径。", "info");
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

if (state.baseUrl && state.baseUrl !== "http://localhost:6701") {
  connect();
} else {
  renderPluginList();
}
