import { useCallback, useEffect, useState } from 'react';
import {
  Bot,
  FolderOpen,
  Inbox,
  ListTodo,
  Lock,
  Moon,
  Search,
  Settings as SettingsIcon,
  Sun,
  Wrench,
} from 'lucide-react';
import {
  api,
  getAuthToken,
  onUnauthorized,
  setAuthToken,
  withTokenQuery,
} from './api';
import { CommandPalette, type NavTab } from './components/CommandPalette';
import { RunTimelineDrawer } from './components/RunTimelineDrawer';
import { InboxPage } from './pages/InboxPage';
import { SettingsPage } from './pages/SettingsPage';
import { TasksPage } from './pages/TasksPage';
import { ToolboxPage } from './pages/ToolboxPage';
import { WorkspacePage } from './pages/WorkspacePage';
import type {
  ApprovalWithContext,
  LlmSettings,
  MemoryItem,
  NotificationChannel,
  RuntimeStatusResponse,
  SecretMeta,
  SkillSummary,
  Task,
  TaskRunWithTask,
  ToolInfo,
  Worker,
} from './types';

export default function App() {
  const [tab, setTab] = useState<NavTab>('inbox');
  const [dark, setDark] = useState<boolean>(() => localStorage.getItem('aiworker_theme') === 'dark');
  const [paletteOpen, setPaletteOpen] = useState(false);

  const [tasks, setTasks] = useState<Task[]>([]);
  const [runs, setRuns] = useState<TaskRunWithTask[]>([]);
  const [approvals, setApprovals] = useState<ApprovalWithContext[]>([]);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [workers, setWorkers] = useState<Worker[]>([]);
  const [memories, setMemories] = useState<MemoryItem[]>([]);
  const [secrets, setSecrets] = useState<SecretMeta[]>([]);
  const [notifications, setNotifications] = useState<NotificationChannel[]>([]);
  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatusResponse | null>(null);
  const [llmSettings, setLlmSettings] = useState<LlmSettings>({
    provider: 'mock',
    base_url: 'https://api.openai.com/v1',
    api_key: '',
    model: 'gpt-4o-mini',
    temperature: 0.2,
    default_max_steps: 12,
  });

  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [authRequired, setAuthRequired] = useState(false);
  const [tokenInput, setTokenInput] = useState(getAuthToken());

  useEffect(() => {
    document.documentElement.classList.toggle('dark', dark);
    localStorage.setItem('aiworker_theme', dark ? 'dark' : 'light');
  }, [dark]);

  // Global Cmd/Ctrl + K shortcut
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 3000);
  }, []);

  const refreshAll = useCallback(async () => {
    try {
      const [t, r, a, tl, sk, wk, m, sec, notif, rt, s] = await Promise.all([
        api.listTasks(),
        api.listRuns(50),
        api.listApprovals('pending'),
        api.listTools(),
        api.listSkills(),
        api.listWorkers(),
        api.listMemories(),
        api.listSecrets(),
        api.listNotifications(),
        api.getRuntimeStatus(),
        api.getLlmSettings(),
      ]);
      setTasks(t);
      setRuns(r);
      setApprovals(a);
      setTools(tl);
      setSkills(sk);
      setWorkers(wk);
      setMemories(m);
      setSecrets(sec);
      setNotifications(notif);
      setRuntimeStatus(rt);
      setLlmSettings(s);
      setAuthRequired(false);
    } catch {
      // Handled by 401 listener or transient startup state
    }
  }, []);

  // Lightweight incremental refresh triggered by SSE events (avoids re-fetching all 11 endpoints)
  const refreshLive = useCallback(async () => {
    try {
      const [r, a] = await Promise.all([api.listRuns(50), api.listApprovals('pending')]);
      setRuns(r);
      setApprovals(a);
    } catch {
      // ignore transient SSE refresh error
    }
  }, []);

  useEffect(() => {
    onUnauthorized(() => setAuthRequired(true));
    refreshAll();

    const es = new EventSource(withTokenQuery('/api/events'));
    const onEvent = () => {
      refreshLive();
    };
    es.addEventListener('status', onEvent);
    es.addEventListener('step', onEvent);
    es.addEventListener('approval_needed', onEvent);
    es.addEventListener('completed', onEvent);
    es.addEventListener('failed', onEvent);
    return () => es.close();
  }, [refreshAll, refreshLive]);

  const handleUnlock = async (e: React.FormEvent) => {
    e.preventDefault();
    setAuthToken(tokenInput);
    await refreshAll();
  };

  const handleQuickDispatch = async (prompt: string) => {
    const res = await api.createTask({ prompt, run_immediately: true });
    await refreshAll();
    if (res.run) {
      setSelectedRunId(res.run.id);
    }
    showToast('已通过快捷指令派发任务');
  };

  const activeRuns = runs.filter(
    (r) => r.status === 'running' || r.status === 'queued' || r.status === 'waiting_approval',
  );

  const navItems: Array<{
    key: NavTab;
    label: string;
    shortLabel: string;
    icon: React.ReactNode;
    badge?: number;
    badgeTone?: 'amber' | 'blue';
  }> = [
    {
      key: 'inbox',
      label: 'Today / 收件箱',
      shortLabel: 'Today',
      icon: <Inbox className="h-4 w-4" />,
      badge: approvals.length > 0 ? approvals.length : undefined,
      badgeTone: 'amber',
    },
    {
      key: 'tasks',
      label: '任务与触发器',
      shortLabel: '任务',
      icon: <ListTodo className="h-4 w-4" />,
      badge: activeRuns.length > 0 ? activeRuns.length : undefined,
      badgeTone: 'blue',
    },
    {
      key: 'workspace',
      label: '工作区文件',
      shortLabel: '文件',
      icon: <FolderOpen className="h-4 w-4" />,
    },
    {
      key: 'toolbox',
      label: '能力中心',
      shortLabel: '能力',
      icon: <Wrench className="h-4 w-4" />,
    },
    {
      key: 'settings',
      label: '模型与设置',
      shortLabel: '设置',
      icon: <SettingsIcon className="h-4 w-4" />,
    },
  ];

  return (
    <div className="flex min-h-screen flex-col bg-slate-50 text-slate-900 md:flex-row dark:bg-slate-950 dark:text-slate-100">
      {/* Mobile Top Header */}
      <header className="sticky top-0 z-30 flex items-center justify-between border-b border-slate-200 bg-white/90 px-4 py-3 backdrop-blur-md md:hidden dark:border-slate-800 dark:bg-slate-900/90">
        <div className="flex items-center gap-2">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-indigo-600 text-white">
            <Bot className="h-4 w-4" />
          </div>
          <span className="text-sm font-bold tracking-tight">aiworker</span>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => setPaletteOpen(true)}
            className="inline-flex items-center gap-1 rounded-lg border border-slate-200 bg-slate-50 px-2.5 py-1.5 text-xs text-slate-600 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
          >
            <Search className="h-3.5 w-3.5" />
            快捷指令
          </button>
          <button
            onClick={() => setDark((d) => !d)}
            className="rounded-lg p-2 text-slate-500 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800"
            aria-label="切换深色模式"
          >
            {dark ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
          </button>
        </div>
      </header>

      {/* Desktop Sidebar */}
      <aside className="hidden w-60 shrink-0 flex-col border-r border-slate-200 bg-white md:flex dark:border-slate-800 dark:bg-slate-900">
        <div className="flex items-center justify-between border-b border-slate-100 px-5 py-4 dark:border-slate-800">
          <div className="flex items-center gap-2.5">
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-indigo-600 text-white shadow-xs">
              <Bot className="h-5 w-5" />
            </div>
            <div>
              <div className="text-sm font-bold tracking-tight">aiworker</div>
              <div className="text-[11px] text-slate-400">Personal AI Automation</div>
            </div>
          </div>
          <button
            onClick={() => setDark((d) => !d)}
            className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-slate-800 dark:hover:text-slate-200"
            title={dark ? '切换浅色' : '切换深色'}
          >
            {dark ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
          </button>
        </div>

        <div className="px-3 pt-3">
          <button
            onClick={() => setPaletteOpen(true)}
            className="flex w-full items-center justify-between rounded-xl border border-slate-200 bg-slate-50 px-3 py-2 text-xs text-slate-500 transition hover:border-indigo-300 hover:text-slate-800 dark:border-slate-700 dark:bg-slate-800/70 dark:text-slate-400 dark:hover:text-slate-200"
          >
            <span className="flex items-center gap-2">
              <Search className="h-3.5 w-3.5" />
              快捷指令...
            </span>
            <kbd className="rounded border border-slate-200 bg-white px-1.5 py-0.5 text-[10px] dark:border-slate-700 dark:bg-slate-900">
              ⌘K
            </kbd>
          </button>
        </div>

        <nav className="flex-1 space-y-1 p-3">
          {navItems.map((item) => (
            <button
              key={item.key}
              onClick={() => setTab(item.key)}
              className={`flex w-full items-center justify-between rounded-xl px-3 py-2 text-sm font-medium transition ${
                tab === item.key
                  ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/70 dark:text-indigo-300'
                  : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900 dark:text-slate-400 dark:hover:bg-slate-800 dark:hover:text-slate-100'
              }`}
            >
              <span className="flex items-center gap-2.5">
                {item.icon}
                {item.label}
              </span>
              {item.badge !== undefined && (
                <span
                  className={`rounded-full px-2 py-0.5 text-xs font-semibold ${
                    item.badgeTone === 'amber'
                      ? 'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300'
                      : 'bg-blue-100 text-blue-700 dark:bg-blue-950 dark:text-blue-300'
                  }`}
                >
                  {item.badge}
                </span>
              )}
            </button>
          ))}
        </nav>

        <div className="space-y-1.5 border-t border-slate-100 p-4 text-xs text-slate-500 dark:border-slate-800 dark:text-slate-400">
          <div className="flex items-center justify-between gap-2">
            <span>引擎</span>
            <span className="truncate rounded bg-slate-100 px-1.5 py-0.5 font-mono text-[11px] text-slate-700 dark:bg-slate-800 dark:text-slate-300">
              {llmSettings.provider === 'mock' ? 'Mock 演示' : llmSettings.model}
            </span>
          </div>
          {runtimeStatus && (
            <div className="flex items-center justify-between">
              <span>沙箱</span>
              <span className="rounded bg-slate-100 px-1.5 py-0.5 font-mono text-[11px] text-slate-700 dark:bg-slate-800 dark:text-slate-300">
                {runtimeStatus.docker_available ? 'Docker' : 'Local'}
              </span>
            </div>
          )}
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 overflow-y-auto pb-24 md:pb-8">
        <div className="mx-auto max-w-5xl px-4 py-6 sm:px-6 sm:py-8">
          {tab === 'inbox' && (
            <InboxPage
              approvals={approvals}
              runs={runs}
              workers={workers}
              todayTokens={runtimeStatus?.today_tokens_used ?? 0}
              onRefresh={refreshAll}
              onSelectRun={setSelectedRunId}
              showToast={showToast}
            />
          )}

          {tab === 'tasks' && (
            <TasksPage
              tasks={tasks}
              workers={workers}
              onRefresh={refreshAll}
              onSelectRun={setSelectedRunId}
              showToast={showToast}
            />
          )}

          {tab === 'workspace' && <WorkspacePage showToast={showToast} />}

          {tab === 'toolbox' && (
            <ToolboxPage
              tools={tools}
              skills={skills}
              workers={workers}
              memories={memories}
              onRefresh={refreshAll}
              showToast={showToast}
            />
          )}

          {tab === 'settings' && (
            <SettingsPage
              llmSettings={llmSettings}
              setLlmSettings={setLlmSettings}
              runtimeStatus={runtimeStatus}
              secrets={secrets}
              notifications={notifications}
              onRefresh={refreshAll}
              showToast={showToast}
            />
          )}
        </div>
      </main>

      {/* Mobile Bottom Tab Bar */}
      <nav className="fixed inset-x-0 bottom-0 z-30 grid grid-cols-5 border-t border-slate-200 bg-white/95 px-1 py-1.5 backdrop-blur-md md:hidden dark:border-slate-800 dark:bg-slate-900/95">
        {navItems.map((item) => (
          <button
            key={item.key}
            onClick={() => setTab(item.key)}
            className={`relative flex flex-col items-center justify-center gap-0.5 rounded-xl py-1.5 text-[11px] font-medium transition ${
              tab === item.key
                ? 'text-indigo-600 dark:text-indigo-400'
                : 'text-slate-500 dark:text-slate-400'
            }`}
          >
            {item.icon}
            <span>{item.shortLabel}</span>
            {item.badge !== undefined && (
              <span
                className={`absolute top-0.5 right-3 min-w-[16px] rounded-full px-1 text-center text-[10px] font-bold text-white ${
                  item.badgeTone === 'amber' ? 'bg-amber-500' : 'bg-blue-500'
                }`}
              >
                {item.badge}
              </span>
            )}
          </button>
        ))}
      </nav>

      <CommandPalette
        open={paletteOpen}
        dark={dark}
        onClose={() => setPaletteOpen(false)}
        onNavigate={setTab}
        onToggleDark={() => setDark((d) => !d)}
        onQuickDispatch={handleQuickDispatch}
      />

      <RunTimelineDrawer
        runId={selectedRunId}
        onClose={() => setSelectedRunId(null)}
        onUpdated={refreshLive}
      />

      {/* Bearer Auth Modal */}
      {authRequired && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 px-4 backdrop-blur-xs">
          <form
            onSubmit={handleUnlock}
            className="w-full max-w-sm space-y-4 rounded-2xl bg-white p-6 shadow-2xl dark:bg-slate-900"
          >
            <div className="flex items-center gap-2 text-slate-900 dark:text-slate-100">
              <Lock className="h-5 w-5 text-indigo-600" />
              <h2 className="text-base font-bold">访问鉴权</h2>
            </div>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              当前服务端已启用 <span className="font-mono">AIWORKER_AUTH_TOKEN</span> 保护，请输入访问令牌解锁工作台。
            </p>
            <input
              type="password"
              value={tokenInput}
              onChange={(e) => setTokenInput(e.target.value)}
              placeholder="输入访问令牌..."
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-sm dark:border-slate-700"
              autoFocus
            />
            <button
              type="submit"
              className="w-full rounded-xl bg-indigo-600 py-2 text-sm font-medium text-white hover:bg-indigo-700"
            >
              解锁工作台
            </button>
          </form>
        </div>
      )}

      {toast && (
        <div className="fixed right-4 bottom-20 z-50 rounded-xl bg-slate-900 px-4 py-2.5 text-sm font-medium text-white shadow-lg md:bottom-5 dark:bg-indigo-600">
          {toast}
        </div>
      )}
    </div>
  );
}
