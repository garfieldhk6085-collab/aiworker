import { useEffect, useState } from 'react';
import {
  FolderOpen,
  Inbox,
  ListTodo,
  Moon,
  Search,
  Send,
  Settings as SettingsIcon,
  Sun,
  Wrench,
} from 'lucide-react';

export type NavTab = 'inbox' | 'tasks' | 'workspace' | 'toolbox' | 'settings';

interface CommandPaletteProps {
  open: boolean;
  dark: boolean;
  onClose: () => void;
  onNavigate: (tab: NavTab) => void;
  onToggleDark: () => void;
  onQuickDispatch: (prompt: string) => Promise<void>;
}

export function CommandPalette({
  open,
  dark,
  onClose,
  onNavigate,
  onToggleDark,
  onQuickDispatch,
}: CommandPaletteProps) {
  const [query, setQuery] = useState('');
  const [sending, setSending] = useState(false);

  useEffect(() => {
    if (!open) setQuery('');
  }, [open]);

  if (!open) return null;

  const actions: Array<{
    id: string;
    label: string;
    hint: string;
    icon: React.ReactNode;
    run: () => void;
  }> = [
    {
      id: 'nav-inbox',
      label: '前往 Today / 审批收件箱',
      hint: '工作台',
      icon: <Inbox className="h-4 w-4" />,
      run: () => {
        onNavigate('inbox');
        onClose();
      },
    },
    {
      id: 'nav-tasks',
      label: '前往 任务与触发器',
      hint: 'Cron & Webhook',
      icon: <ListTodo className="h-4 w-4" />,
      run: () => {
        onNavigate('tasks');
        onClose();
      },
    },
    {
      id: 'nav-workspace',
      label: '前往 工作区文件',
      hint: '上传 / 下载产物',
      icon: <FolderOpen className="h-4 w-4" />,
      run: () => {
        onNavigate('workspace');
        onClose();
      },
    },
    {
      id: 'nav-toolbox',
      label: '前往 能力中心',
      hint: '工具 / Skills / Workers / 记忆',
      icon: <Wrench className="h-4 w-4" />,
      run: () => {
        onNavigate('toolbox');
        onClose();
      },
    },
    {
      id: 'nav-settings',
      label: '前往 模型与系统设置',
      hint: 'LLM / 沙箱 / S3 备份',
      icon: <SettingsIcon className="h-4 w-4" />,
      run: () => {
        onNavigate('settings');
        onClose();
      },
    },
    {
      id: 'toggle-theme',
      label: dark ? '切换为浅色模式' : '切换为深色模式',
      hint: '主题',
      icon: dark ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />,
      run: () => {
        onToggleDark();
        onClose();
      },
    },
  ];

  const trimmed = query.trim();
  const filtered = trimmed
    ? actions.filter(
        (a) =>
          a.label.toLowerCase().includes(trimmed.toLowerCase()) ||
          a.hint.toLowerCase().includes(trimmed.toLowerCase()),
      )
    : actions;

  const handleDispatch = async () => {
    if (!trimmed || sending) return;
    setSending(true);
    try {
      await onQuickDispatch(trimmed);
      onClose();
    } finally {
      setSending(false);
    }
  };

  return (
    <div
      onClick={onClose}
      className="fixed inset-0 z-50 flex items-start justify-center bg-slate-900/45 px-4 pt-20 backdrop-blur-[2px]"
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-lg overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-2xl dark:border-slate-700 dark:bg-slate-900"
      >
        <div className="flex items-center gap-2.5 border-b border-slate-100 px-4 py-3 dark:border-slate-800">
          <Search className="h-4 w-4 shrink-0 text-slate-400" />
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape') onClose();
              if (e.key === 'Enter' && trimmed) {
                e.preventDefault();
                handleDispatch();
              }
            }}
            placeholder="输入自然语言直接派发任务，或搜索页面与操作..."
            className="w-full bg-transparent text-sm text-slate-900 placeholder:text-slate-400 focus:outline-none dark:text-slate-100"
          />
          <kbd className="hidden rounded border border-slate-200 px-1.5 py-0.5 text-[10px] text-slate-400 sm:inline dark:border-slate-700">
            ESC
          </kbd>
        </div>

        <div className="max-h-80 overflow-y-auto p-2">
          {trimmed && (
            <button
              disabled={sending}
              onClick={handleDispatch}
              className="mb-1 flex w-full items-center justify-between gap-2 rounded-xl bg-indigo-50 px-3 py-2.5 text-left text-sm font-medium text-indigo-700 hover:bg-indigo-100 dark:bg-indigo-950/60 dark:text-indigo-300 dark:hover:bg-indigo-950"
            >
              <span className="flex min-w-0 items-center gap-2.5">
                <Send className="h-4 w-4 shrink-0" />
                <span className="truncate">立即派发任务：「{trimmed}」</span>
              </span>
              <span className="shrink-0 text-[11px] opacity-75">Enter</span>
            </button>
          )}

          {filtered.map((item) => (
            <button
              key={item.id}
              onClick={item.run}
              className="flex w-full items-center justify-between rounded-xl px-3 py-2.5 text-left text-sm text-slate-700 hover:bg-slate-100 dark:text-slate-200 dark:hover:bg-slate-800"
            >
              <span className="flex items-center gap-2.5">
                <span className="text-slate-400">{item.icon}</span>
                {item.label}
              </span>
              <span className="text-xs text-slate-400">{item.hint}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
