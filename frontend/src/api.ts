import type {
  ApprovalWithContext,
  BackupSettings,
  LlmSettings,
  MemoryItem,
  NotificationChannel,
  RestoreResult,
  RunDetail,
  RuntimeSettings,
  RuntimeStatusResponse,
  S3BackupObject,
  SecretMeta,
  SkillDetail,
  SkillSummary,
  Task,
  TaskRun,
  TaskRunWithTask,
  ToolInfo,
  ToolPolicy,
  Worker,
  WorkspaceEntry,
} from './types';

const AUTH_STORAGE_KEY = 'aiworker_auth_token';

export function getAuthToken(): string {
  return localStorage.getItem(AUTH_STORAGE_KEY) ?? '';
}

export function setAuthToken(token: string) {
  const trimmed = token.trim();
  if (trimmed) {
    localStorage.setItem(AUTH_STORAGE_KEY, trimmed);
  } else {
    localStorage.removeItem(AUTH_STORAGE_KEY);
  }
}

export function withTokenQuery(url: string): string {
  const token = getAuthToken();
  if (!token) return url;
  const sep = url.includes('?') ? '&' : '?';
  return `${url}${sep}token=${encodeURIComponent(token)}`;
}

export class UnauthorizedError extends Error {
  constructor() {
    super('Unauthorized');
    this.name = 'UnauthorizedError';
  }
}

let unauthorizedListener: (() => void) | null = null;
export function onUnauthorized(cb: () => void) {
  unauthorizedListener = cb;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const token = getAuthToken();
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...((init?.headers as Record<string, string>) ?? {}),
  };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const res = await fetch(path, {
    ...init,
    headers,
  });

  if (res.status === 401) {
    unauthorizedListener?.();
    throw new UnauthorizedError();
  }

  if (!res.ok) {
    let message = `HTTP ${res.status}`;
    try {
      const data = await res.json();
      if (data?.error) message = data.error;
    } catch {
      // ignore json parse error
    }
    throw new Error(message);
  }
  return res.json() as Promise<T>;
}

export const api = {
  // Tasks
  listTasks: () => request<Task[]>('/api/tasks'),
  createTask: (body: {
    title?: string;
    prompt: string;
    schedule_type?: 'once' | 'cron';
    cron_expr?: string;
    max_steps?: number;
    worker_id?: string;
    enable_webhook?: boolean;
    run_immediately?: boolean;
  }) =>
    request<{ task: Task; run: TaskRun | null }>('/api/tasks', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  updateTask: (
    id: string,
    body: Partial<{
      title: string;
      prompt: string;
      schedule_type: 'once' | 'cron';
      cron_expr: string;
      enabled: boolean;
      max_steps: number;
      worker_id: string;
      regenerate_webhook: boolean;
    }>,
  ) =>
    request<Task>(`/api/tasks/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(body),
    }),
  deleteTask: (id: string) =>
    request<{ deleted: boolean }>(`/api/tasks/${id}`, { method: 'DELETE' }),
  triggerTask: (id: string) =>
    request<TaskRun>(`/api/tasks/${id}/run`, { method: 'POST' }),

  // Runs
  listRuns: (limit = 50) => request<TaskRunWithTask[]>(`/api/runs?limit=${limit}`),
  getRunDetail: (id: string) => request<RunDetail>(`/api/runs/${id}`),
  cancelRun: (id: string) =>
    request<TaskRun>(`/api/runs/${id}/cancel`, { method: 'POST' }),

  // Approvals
  listApprovals: (status?: string) =>
    request<ApprovalWithContext[]>(
      status ? `/api/approvals?status=${encodeURIComponent(status)}` : '/api/approvals',
    ),
  decideApproval: (id: string, decision: 'approved' | 'rejected', feedback?: string) =>
    request<TaskRun>(`/api/approvals/${id}/decide`, {
      method: 'POST',
      body: JSON.stringify({ decision, feedback }),
    }),

  // Tools
  listTools: () => request<ToolInfo[]>('/api/tools'),
  updateToolPolicy: (name: string, policy: ToolPolicy) =>
    request<{ name: string; policy: ToolPolicy }>(`/api/tools/${name}/policy`, {
      method: 'PATCH',
      body: JSON.stringify({ policy }),
    }),

  // Skills
  listSkills: () => request<SkillSummary[]>('/api/skills'),
  getSkill: (name: string) => request<SkillDetail>(`/api/skills/${encodeURIComponent(name)}`),
  upsertSkill: (body: { name: string; description: string; content: string }) =>
    request<SkillDetail>('/api/skills', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  deleteSkill: (name: string) =>
    request<{ deleted: boolean }>(`/api/skills/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }),

  // Workers
  listWorkers: () => request<Worker[]>('/api/workers'),
  createWorker: (body: {
    name: string;
    description?: string;
    system_prompt: string;
    provider?: string;
    model?: string;
    base_url?: string;
    allowed_tools?: string[];
    max_steps?: number;
  }) =>
    request<Worker>('/api/workers', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  deleteWorker: (id: string) =>
    request<{ deleted: boolean }>(`/api/workers/${id}`, { method: 'DELETE' }),

  // Memories
  listMemories: () => request<MemoryItem[]>('/api/memories'),
  upsertMemory: (body: { key: string; content: string; category: string }) =>
    request<MemoryItem>('/api/memories', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  deleteMemory: (id: string) =>
    request<{ deleted: boolean }>(`/api/memories/${id}`, { method: 'DELETE' }),

  // Secrets Vault
  listSecrets: () => request<SecretMeta[]>('/api/secrets'),
  upsertSecret: (body: { name: string; value: string; description?: string }) =>
    request<{ saved: boolean; name: string }>('/api/secrets', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  deleteSecret: (id: string) =>
    request<{ deleted: boolean }>(`/api/secrets/${id}`, { method: 'DELETE' }),

  // Notifications
  listNotifications: () => request<NotificationChannel[]>('/api/notifications'),
  createNotification: (body: {
    name: string;
    channel_type: 'webhook' | 'feishu' | 'slack' | 'discord';
    target_url: string;
    events?: string[];
    enabled?: boolean;
  }) =>
    request<NotificationChannel>('/api/notifications', {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  updateNotification: (
    id: string,
    body: Partial<{
      name: string;
      channel_type: string;
      target_url: string;
      events: string[];
      enabled: boolean;
    }>,
  ) =>
    request<NotificationChannel>(`/api/notifications/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(body),
    }),
  deleteNotification: (id: string) =>
    request<{ deleted: boolean }>(`/api/notifications/${id}`, { method: 'DELETE' }),
  testNotification: (id: string) =>
    request<{ sent: boolean }>(`/api/notifications/${id}/test`, { method: 'POST' }),

  // Workspace Files
  listWorkspaceFiles: (path = '') =>
    request<WorkspaceEntry[]>(
      path ? `/api/workspace/files?path=${encodeURIComponent(path)}` : '/api/workspace/files',
    ),
  uploadWorkspaceFile: (path: string, content: string, isBase64 = false) =>
    request<{ uploaded: boolean; path: string; bytes: number }>('/api/workspace/upload', {
      method: 'POST',
      body: JSON.stringify({ path, content, is_base64: isBase64 }),
    }),
  deleteWorkspaceFile: (path: string) =>
    request<{ deleted: boolean }>(`/api/workspace/files?path=${encodeURIComponent(path)}`, {
      method: 'DELETE',
    }),
  downloadWorkspaceUrl: (path: string) =>
    withTokenQuery(`/api/workspace/download?path=${encodeURIComponent(path)}`),

  // Backup & S3 Sync
  getBackupSettings: () => request<BackupSettings>('/api/backup/settings'),
  saveBackupSettings: (settings: BackupSettings) =>
    request<BackupSettings>('/api/backup/settings', {
      method: 'PUT',
      body: JSON.stringify(settings),
    }),
  exportLocalBackupUrl: () => withTokenQuery('/api/backup/export'),
  pushS3Backup: () =>
    request<{ uploaded: boolean; key: string; size_bytes: number; timestamp: string }>(
      '/api/backup/s3/push',
      { method: 'POST' },
    ),
  listS3Backups: () => request<S3BackupObject[]>('/api/backup/s3/list'),
  restoreS3Backup: (key: string) =>
    request<RestoreResult>('/api/backup/s3/restore', {
      method: 'POST',
      body: JSON.stringify({ key }),
    }),

  // Settings
  getLlmSettings: () => request<LlmSettings>('/api/settings/llm'),
  saveLlmSettings: (settings: LlmSettings) =>
    request<LlmSettings>('/api/settings/llm', {
      method: 'PUT',
      body: JSON.stringify(settings),
    }),
  getRuntimeStatus: () => request<RuntimeStatusResponse>('/api/settings/runtime'),
  saveRuntimeSettings: (settings: RuntimeSettings) =>
    request<RuntimeSettings>('/api/settings/runtime', {
      method: 'PUT',
      body: JSON.stringify(settings),
    }),
};
