import { useState } from 'react';
import {
  Activity,
  Bot,
  Clock,
  Send,
  ShieldCheck,
  Sparkles,
  Webhook,
  Zap,
} from 'lucide-react';
import { api } from '../api';
import { ApprovalCard } from '../components/ApprovalCard';
import { StatusBadge } from '../components/StatusBadge';
import type { ApprovalWithContext, TaskRunWithTask, Worker } from '../types';

interface InboxPageProps {
  approvals: ApprovalWithContext[];
  runs: TaskRunWithTask[];
  workers: Worker[];
  todayTokens: number;
  onRefresh: () => Promise<void>;
  onSelectRun: (runId: string) => void;
  showToast: (msg: string) => void;
}

const QUICK_TEMPLATES = [
  {
    label: '每日工作区简报',
    prompt: '检查本地 workspace 中的最新笔记与文件，整理一份今日重点行动简报并保存为 Markdown。',
  },
  {
    label: 'Python 数据分析',
    prompt: '扫描 workspace 中的数据文件，使用 python_exec 统计关键指标并生成分析报告。',
  },
  {
    label: '网页要点提炼',
    prompt: '抓取我指定的网页正文，提炼 5 条核心结论和行动建议。',
  },
];

export function InboxPage({
  approvals,
  runs,
  workers,
  todayTokens,
  onRefresh,
  onSelectRun,
  showToast,
}: InboxPageProps) {
  const [prompt, setPrompt] = useState('');
  const [scheduleType, setScheduleType] = useState<'once' | 'cron'>('once');
  const [cronExpr, setCronExpr] = useState('0 9 * * *');
  const [workerId, setWorkerId] = useState('');
  const [enableWebhook, setEnableWebhook] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  const runningCount = runs.filter((r) => r.status === 'running' || r.status === 'queued').length;

  const submitTask = async () => {
    if (!prompt.trim() || submitting) return;
    setSubmitting(true);
    try {
      const res = await api.createTask({
        prompt: prompt.trim(),
        schedule_type: scheduleType,
        cron_expr: scheduleType === 'cron' ? cronExpr.trim() : undefined,
        worker_id: workerId || undefined,
        enable_webhook: enableWebhook,
        run_immediately: true,
      });
      setPrompt('');
      setEnableWebhook(false);
      await onRefresh();
      if (res.run) {
        onSelectRun(res.run.id);
      }
      showToast('任务已派发给 AI Worker');
    } catch (err) {
      showToast(`创建失败: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setSubmitting(false);
    }
  };

  const handleDispatch = async (e: React.FormEvent) => {
    e.preventDefault();
    await submitTask();
  };

  const handleDecide = async (
    id: string,
    decision: 'approved' | 'rejected',
    feedback?: string,
  ) => {
    await api.decideApproval(id, decision, feedback);
    await onRefresh();
    showToast(decision === 'approved' ? '已批准，任务继续执行' : '已驳回并通知 AI Worker');
  };

  return (
    <div className="space-y-6 sm:space-y-8">
      {/* Header + Compact KPI Strip */}
      <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
        <div>
          <h1 className="text-2xl font-bold tracking-tight text-slate-900 dark:text-slate-100">
            Today 工作台
          </h1>
          <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
            自然语言派发任务，敏感操作自动停在下方等你确认（Draft-first）。
          </p>
        </div>

        <div className="grid grid-cols-3 gap-2 sm:flex sm:items-center">
          <StatPill label="待审批" value={approvals.length} highlight={approvals.length > 0} />
          <StatPill label="运行中" value={runningCount} />
          <StatPill label="今日 Token" value={todayTokens.toLocaleString()} />
        </div>
      </div>

      {/* Command Bar */}
      <form
        onSubmit={handleDispatch}
        className="rounded-2xl border border-slate-200 bg-white p-4 shadow-xs dark:border-slate-800 dark:bg-slate-900"
      >
        <div className="flex items-start gap-3">
          <Sparkles className="mt-2 h-5 w-5 shrink-0 text-indigo-500" />
          <textarea
            rows={2}
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            onKeyDown={(e) => {
              if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
                e.preventDefault();
                submitTask();
              }
            }}
            placeholder="输入任务目标（Cmd/Ctrl + Enter 立即派发）..."
            className="w-full resize-none border-0 bg-transparent text-sm text-slate-900 placeholder:text-slate-400 focus:outline-none dark:text-slate-100"
          />
        </div>

        {/* Quick Templates */}
        <div className="mt-2.5 flex flex-wrap gap-1.5">
          {QUICK_TEMPLATES.map((tpl) => (
            <button
              key={tpl.label}
              type="button"
              onClick={() => setPrompt(tpl.prompt)}
              className="inline-flex items-center gap-1 rounded-full border border-slate-200 bg-slate-50 px-2.5 py-1 text-[11px] font-medium text-slate-600 transition hover:border-indigo-200 hover:bg-indigo-50 hover:text-indigo-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300 dark:hover:border-indigo-800 dark:hover:bg-indigo-950/60"
            >
              <Zap className="h-3 w-3 text-indigo-500" />
              {tpl.label}
            </button>
          ))}
        </div>

        <div className="mt-3 flex flex-wrap items-center justify-between gap-3 border-t border-slate-100 pt-3 dark:border-slate-800">
          <div className="flex flex-wrap items-center gap-1.5 text-xs">
            <button
              type="button"
              onClick={() => setScheduleType('once')}
              className={`rounded-lg px-2.5 py-1 font-medium transition ${
                scheduleType === 'once'
                  ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/70 dark:text-indigo-300'
                  : 'text-slate-500 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800'
              }`}
            >
              一次性
            </button>
            <button
              type="button"
              onClick={() => setScheduleType('cron')}
              className={`inline-flex items-center gap-1 rounded-lg px-2.5 py-1 font-medium transition ${
                scheduleType === 'cron'
                  ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/70 dark:text-indigo-300'
                  : 'text-slate-500 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800'
              }`}
            >
              <Clock className="h-3.5 w-3.5" />
              Cron
            </button>
            {scheduleType === 'cron' && (
              <input
                type="text"
                value={cronExpr}
                onChange={(e) => setCronExpr(e.target.value)}
                placeholder="0 9 * * *"
                className="w-28 rounded-lg border border-slate-200 bg-transparent px-2 py-1 font-mono text-xs text-slate-700 outline-none focus:border-indigo-400 dark:border-slate-700 dark:text-slate-200"
              />
            )}

            <button
              type="button"
              onClick={() => setEnableWebhook((v) => !v)}
              className={`inline-flex items-center gap-1 rounded-lg px-2.5 py-1 font-medium transition ${
                enableWebhook
                  ? 'bg-emerald-50 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300'
                  : 'text-slate-500 hover:bg-slate-100 dark:text-slate-400 dark:hover:bg-slate-800'
              }`}
            >
              <Webhook className="h-3.5 w-3.5" />
              Webhook
            </button>

            {workers.length > 0 && (
              <div className="inline-flex items-center gap-1 rounded-lg border border-slate-200 bg-slate-50 px-2 py-1 dark:border-slate-700 dark:bg-slate-800">
                <Bot className="h-3.5 w-3.5 text-slate-500" />
                <select
                  value={workerId}
                  onChange={(e) => setWorkerId(e.target.value)}
                  className="bg-transparent text-xs text-slate-700 outline-none dark:text-slate-200"
                >
                  <option value="">默认助手</option>
                  {workers.map((w) => (
                    <option key={w.id} value={w.id}>
                      {w.name}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </div>

          <button
            type="submit"
            disabled={submitting || !prompt.trim()}
            className="inline-flex w-full items-center justify-center gap-1.5 rounded-xl bg-indigo-600 px-4 py-2 text-sm font-medium text-white shadow-xs transition hover:bg-indigo-700 disabled:opacity-50 sm:w-auto"
          >
            <Send className="h-4 w-4" />
            派发任务
          </button>
        </div>
      </form>

      {/* Pending Approvals */}
      <section className="space-y-3">
        <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-800 dark:text-slate-200">
          <ShieldCheck className="h-4 w-4 text-amber-600" />
          待审批动作 ({approvals.length})
        </h2>
        {approvals.length === 0 ? (
          <div className="rounded-xl border border-dashed border-slate-200 bg-white/70 p-8 text-center text-sm text-slate-400 dark:border-slate-800 dark:bg-slate-900/50">
            当前没有等待你确认的敏感操作。点击上方模板或按 Cmd/Ctrl+K 快速派发！
          </div>
        ) : (
          <div className="space-y-3">
            {approvals.map((item) => (
              <ApprovalCard
                key={item.id}
                item={item}
                onDecide={handleDecide}
                onOpenRun={onSelectRun}
              />
            ))}
          </div>
        )}
      </section>

      {/* Recent Runs */}
      <section className="space-y-3">
        <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-800 dark:text-slate-200">
          <Activity className="h-4 w-4 text-indigo-600 dark:text-indigo-400" />
          最近执行动态
        </h2>
        {runs.length === 0 ? (
          <div className="rounded-xl border border-slate-200 bg-white p-6 text-center text-sm text-slate-400 dark:border-slate-800 dark:bg-slate-900">
            暂无执行记录
          </div>
        ) : (
          <div className="divide-y divide-slate-100 overflow-hidden rounded-xl border border-slate-200 bg-white dark:divide-slate-800 dark:border-slate-800 dark:bg-slate-900">
            {runs.slice(0, 10).map((run) => (
              <div
                key={run.id}
                onClick={() => onSelectRun(run.id)}
                className="flex cursor-pointer items-center justify-between gap-3 px-4 py-3.5 transition hover:bg-slate-50 dark:hover:bg-slate-800/60"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-1.5">
                    <span className="truncate font-medium text-slate-900 dark:text-slate-100">
                      {run.task_title}
                    </span>
                    <StatusBadge status={run.status} />
                    <span className="rounded bg-slate-100 px-1.5 py-0.5 text-[10px] text-slate-500 dark:bg-slate-800 dark:text-slate-400">
                      {run.trigger_type}
                    </span>
                  </div>
                  <p className="mt-0.5 truncate text-xs text-slate-500 dark:text-slate-400">
                    {run.output_summary ?? run.error_message ?? run.task_prompt}
                  </p>
                </div>
                <div className="shrink-0 text-right text-xs text-slate-400">
                  <div>{run.total_tokens} tok</div>
                  <div>{new Date(run.started_at).toLocaleTimeString()}</div>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function StatPill({
  label,
  value,
  highlight = false,
}: {
  label: string;
  value: string | number;
  highlight?: boolean;
}) {
  return (
    <div
      className={`rounded-xl border px-3 py-1.5 text-center sm:text-left ${
        highlight
          ? 'border-amber-200 bg-amber-50/80 dark:border-amber-900 dark:bg-amber-950/40'
          : 'border-slate-200 bg-white dark:border-slate-800 dark:bg-slate-900'
      }`}
    >
      <div className="text-[10px] font-medium text-slate-500 dark:text-slate-400">{label}</div>
      <div
        className={`font-mono text-sm font-bold ${
          highlight
            ? 'text-amber-700 dark:text-amber-300'
            : 'text-slate-900 dark:text-slate-100'
        }`}
      >
        {value}
      </div>
    </div>
  );
}
