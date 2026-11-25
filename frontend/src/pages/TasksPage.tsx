import { Bot, Clock, Copy, Play, Trash2, Webhook } from 'lucide-react';
import { api } from '../api';
import type { Task, Worker } from '../types';

interface TasksPageProps {
  tasks: Task[];
  workers: Worker[];
  onRefresh: () => Promise<void>;
  onSelectRun: (runId: string) => void;
  showToast: (msg: string) => void;
}

export function TasksPage({
  tasks,
  workers,
  onRefresh,
  onSelectRun,
  showToast,
}: TasksPageProps) {
  const workerMap = new Map(workers.map((w) => [w.id, w.name]));

  const copyWebhookUrl = async (token: string) => {
    const url = `${window.location.origin}/api/hooks/${token}`;
    try {
      await navigator.clipboard.writeText(url);
      showToast('Webhook 触发地址已复制');
    } catch {
      showToast(url);
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight text-slate-900">任务与触发器</h1>
        <p className="mt-1 text-sm text-slate-500">
          管理一次性任务、Cron 定时自动化以及 Inbound Webhook 外部触发地址（可用于 Siri 快捷指令 / GitHub Webhook）。
        </p>
      </div>

      <div className="space-y-3">
        {tasks.length === 0 ? (
          <div className="rounded-xl border border-dashed border-slate-200 bg-white p-8 text-center text-sm text-slate-400">
            还没有创建任务，前往 Today 工作台下达第一个目标吧。
          </div>
        ) : (
          tasks.map((task) => (
            <div
              key={task.id}
              className="rounded-xl border border-slate-200 bg-white p-4 shadow-2xs"
            >
              <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-semibold text-slate-900">{task.title}</span>
                    {task.schedule_type === 'cron' ? (
                      <span className="inline-flex items-center gap-1 rounded-full bg-indigo-50 px-2 py-0.5 font-mono text-xs text-indigo-700">
                        <Clock className="h-3 w-3" />
                        {task.cron_expr}
                      </span>
                    ) : (
                      <span className="rounded-full bg-slate-100 px-2 py-0.5 text-xs text-slate-600">
                        一次性
                      </span>
                    )}
                    {task.worker_id && workerMap.has(task.worker_id) && (
                      <span className="inline-flex items-center gap-1 rounded-full bg-violet-50 px-2 py-0.5 text-xs text-violet-700">
                        <Bot className="h-3 w-3" />
                        {workerMap.get(task.worker_id)}
                      </span>
                    )}
                  </div>
                  <p className="mt-1.5 text-xs leading-relaxed text-slate-500">{task.prompt}</p>
                  {task.next_run_at && (
                    <p className="mt-1 text-[11px] text-indigo-600">
                      下次自动执行：{new Date(task.next_run_at).toLocaleString()}
                    </p>
                  )}
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  {task.schedule_type === 'cron' && (
                    <button
                      onClick={async () => {
                        await api.updateTask(task.id, { enabled: task.enabled === 0 });
                        await onRefresh();
                      }}
                      className={`rounded-lg border px-2.5 py-1.5 text-xs font-medium ${
                        task.enabled
                          ? 'border-emerald-200 bg-emerald-50 text-emerald-700'
                          : 'border-slate-200 bg-slate-50 text-slate-500'
                      }`}
                    >
                      {task.enabled ? '定时已开启' : '已暂停'}
                    </button>
                  )}

                  {task.webhook_token ? (
                    <button
                      onClick={() => copyWebhookUrl(task.webhook_token!)}
                      className="inline-flex items-center gap-1 rounded-lg border border-emerald-200 bg-emerald-50 px-2.5 py-1.5 text-xs font-medium text-emerald-700 hover:bg-emerald-100"
                    >
                      <Copy className="h-3.5 w-3.5" />
                      复制 Webhook
                    </button>
                  ) : (
                    <button
                      onClick={async () => {
                        await api.updateTask(task.id, { regenerate_webhook: true });
                        await onRefresh();
                        showToast('已生成 Inbound Webhook 触发器');
                      }}
                      className="inline-flex items-center gap-1 rounded-lg border border-slate-200 bg-white px-2.5 py-1.5 text-xs font-medium text-slate-600 hover:bg-slate-50"
                    >
                      <Webhook className="h-3.5 w-3.5" />
                      开启 Webhook
                    </button>
                  )}

                  <button
                    onClick={async () => {
                      const run = await api.triggerTask(task.id);
                      await onRefresh();
                      onSelectRun(run.id);
                    }}
                    className="inline-flex items-center gap-1 rounded-lg bg-indigo-50 px-3 py-1.5 text-xs font-medium text-indigo-700 hover:bg-indigo-100"
                  >
                    <Play className="h-3.5 w-3.5" />
                    立即运行
                  </button>

                  <button
                    onClick={async () => {
                      await api.deleteTask(task.id);
                      await onRefresh();
                      showToast('任务已删除');
                    }}
                    className="rounded-lg p-1.5 text-slate-400 hover:bg-rose-50 hover:text-rose-600"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>

              {task.webhook_token && (
                <div className="mt-3 flex items-center justify-between rounded-lg bg-slate-50 px-3 py-2 font-mono text-[11px] text-slate-600">
                  <span className="truncate">
                    POST {window.location.origin}/api/hooks/{task.webhook_token}
                  </span>
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
