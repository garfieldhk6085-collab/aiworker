import { useEffect, useState } from 'react';
import { CheckCircle2, Container } from 'lucide-react';
import { api } from '../../api';
import type { RuntimeSettings, RuntimeStatusResponse } from '../../types';

interface Props {
  runtimeStatus: RuntimeStatusResponse | null;
  onRefresh: () => Promise<void>;
  showToast: (msg: string) => void;
}

export function RuntimeSettingsCard({ runtimeStatus, onRefresh, showToast }: Props) {
  const [form, setForm] = useState<RuntimeSettings>(
    runtimeStatus?.settings ?? {
      sandbox_mode: 'auto',
      docker_image: 'python:3.12-slim',
      docker_network: 'none',
      default_timeout_secs: 30,
      max_concurrent_runs: 4,
      daily_token_budget: 500000,
      retention_days: 30,
    },
  );

  useEffect(() => {
    if (runtimeStatus?.settings) {
      setForm(runtimeStatus.settings);
    }
  }, [runtimeStatus]);

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    await api.saveRuntimeSettings(form);
    await onRefresh();
    showToast('沙箱与运行策略已保存');
  };

  const usedTokens = runtimeStatus?.today_tokens_used ?? 0;
  const budget = form.daily_token_budget;
  const budgetPct = budget > 0 ? Math.min(100, Math.round((usedTokens / budget) * 100)) : 0;

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="flex items-center gap-2 text-base font-bold text-slate-900 dark:text-slate-100">
          <Container className="h-5 w-5 text-indigo-600 dark:text-indigo-400" />
          Python / Shell 沙箱与预算保护
        </h2>
        {runtimeStatus && (
          <span
            className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${
              runtimeStatus.docker_available
                ? 'bg-emerald-50 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300'
                : 'bg-slate-100 text-slate-600 dark:bg-slate-800 dark:text-slate-300'
            }`}
          >
            Docker: {runtimeStatus.docker_available ? '可用' : '未检测到（自动降级受控本地）'}
          </span>
        )}
      </div>

      <form
        onSubmit={handleSave}
        className="space-y-4 rounded-2xl border border-slate-200 bg-white p-5 sm:p-6 dark:border-slate-800 dark:bg-slate-900"
      >
        {budget > 0 && (
          <div className="rounded-xl bg-slate-50 p-3 dark:bg-slate-800/70">
            <div className="flex justify-between text-xs text-slate-600 dark:text-slate-300">
              <span>今日已消耗 Token</span>
              <span className="font-mono">
                {usedTokens.toLocaleString()} / {budget.toLocaleString()} ({budgetPct}%)
              </span>
            </div>
            <div className="mt-1.5 h-2 overflow-hidden rounded-full bg-slate-200 dark:bg-slate-700">
              <div
                className={`h-full rounded-full ${budgetPct > 85 ? 'bg-rose-500' : 'bg-indigo-500'}`}
                style={{ width: `${budgetPct}%` }}
              />
            </div>
          </div>
        )}

        <div className="grid gap-4 sm:grid-cols-3">
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              沙箱模式
            </label>
            <select
              value={form.sandbox_mode}
              onChange={(e) =>
                setForm({
                  ...form,
                  sandbox_mode: e.target.value as RuntimeSettings['sandbox_mode'],
                })
              }
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-sm dark:border-slate-700"
            >
              <option value="auto">Auto（Docker 优先）</option>
              <option value="docker">仅 Docker</option>
              <option value="local">受控本地进程</option>
            </select>
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              Docker 镜像
            </label>
            <input
              value={form.docker_image}
              onChange={(e) => setForm({ ...form, docker_image: e.target.value })}
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-sm dark:border-slate-700"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              Docker 网络
            </label>
            <select
              value={form.docker_network}
              onChange={(e) =>
                setForm({
                  ...form,
                  docker_network: e.target.value as RuntimeSettings['docker_network'],
                })
              }
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-sm dark:border-slate-700"
            >
              <option value="none">断网（更安全）</option>
              <option value="bridge">Bridge（允许联网）</option>
            </select>
          </div>
        </div>

        <div className="grid gap-4 sm:grid-cols-4">
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              超时（秒）
            </label>
            <input
              type="number"
              min="1"
              max="120"
              value={form.default_timeout_secs}
              onChange={(e) =>
                setForm({ ...form, default_timeout_secs: parseInt(e.target.value, 10) || 30 })
              }
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-sm dark:border-slate-700"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              最大并发任务
            </label>
            <input
              type="number"
              min="1"
              max="32"
              value={form.max_concurrent_runs}
              onChange={(e) =>
                setForm({ ...form, max_concurrent_runs: parseInt(e.target.value, 10) || 4 })
              }
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-sm dark:border-slate-700"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              每日 Token 熔断
            </label>
            <input
              type="number"
              min="0"
              step="10000"
              value={form.daily_token_budget}
              onChange={(e) =>
                setForm({ ...form, daily_token_budget: parseInt(e.target.value, 10) || 0 })
              }
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-sm dark:border-slate-700"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              日志保留天数
            </label>
            <input
              type="number"
              min="0"
              value={form.retention_days}
              onChange={(e) =>
                setForm({ ...form, retention_days: parseInt(e.target.value, 10) || 30 })
              }
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-sm dark:border-slate-700"
            />
          </div>
        </div>

        <button
          type="submit"
          className="inline-flex items-center gap-1.5 rounded-xl bg-indigo-600 px-4 py-2 text-sm font-medium text-white shadow-xs hover:bg-indigo-700"
        >
          <CheckCircle2 className="h-4 w-4" />
          保存沙箱与预算策略
        </button>
      </form>
    </section>
  );
}
