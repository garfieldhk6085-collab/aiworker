import { useState } from 'react';
import { Bell, KeyRound, Plus, Send, Trash2 } from 'lucide-react';
import { api } from '../../api';
import type { NotificationChannel, SecretMeta } from '../../types';

interface Props {
  secrets: SecretMeta[];
  notifications: NotificationChannel[];
  onRefresh: () => Promise<void>;
  showToast: (msg: string) => void;
}

export function SecretsAndNotifyCard({
  secrets,
  notifications,
  onRefresh,
  showToast,
}: Props) {
  const [secName, setSecName] = useState('');
  const [secValue, setSecValue] = useState('');
  const [secDesc, setSecDesc] = useState('');

  const [notifName, setNotifName] = useState('');
  const [notifType, setNotifType] = useState<'webhook' | 'feishu' | 'slack' | 'discord'>('feishu');
  const [notifUrl, setNotifUrl] = useState('');

  const handleAddSecret = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!secName.trim() || !secValue) return;
    try {
      await api.upsertSecret({
        name: secName.trim().toUpperCase(),
        value: secValue,
        description: secDesc.trim(),
      });
      setSecName('');
      setSecValue('');
      setSecDesc('');
      await onRefresh();
      showToast('Secret 已安全存入保险箱');
    } catch (err) {
      showToast(`保存失败: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  const handleAddNotification = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!notifName.trim() || !notifUrl.trim()) return;
    await api.createNotification({
      name: notifName.trim(),
      channel_type: notifType,
      target_url: notifUrl.trim(),
    });
    setNotifName('');
    setNotifUrl('');
    await onRefresh();
    showToast('通知渠道已添加');
  };

  return (
    <div className="space-y-8">
      {/* Secrets Vault */}
      <section className="space-y-3">
        <div>
          <h2 className="flex items-center gap-2 text-base font-bold text-slate-900 dark:text-slate-100">
            <KeyRound className="h-5 w-5 text-indigo-600 dark:text-indigo-400" />
            Secrets 密钥保险箱
          </h2>
          <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
            仅在执行 Python / Shell 沙箱时注入环境变量，输出日志会自动打码脱敏。
          </p>
        </div>

        <form
          onSubmit={handleAddSecret}
          className="grid gap-2.5 rounded-xl border border-slate-200 bg-white p-4 sm:grid-cols-12 dark:border-slate-800 dark:bg-slate-900"
        >
          <input
            value={secName}
            onChange={(e) => setSecName(e.target.value)}
            placeholder="名称（如 GITHUB_TOKEN）"
            className="rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs sm:col-span-4 dark:border-slate-700"
          />
          <input
            type="password"
            value={secValue}
            onChange={(e) => setSecValue(e.target.value)}
            placeholder="Secret 明文值"
            className="rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs sm:col-span-4 dark:border-slate-700"
          />
          <input
            value={secDesc}
            onChange={(e) => setSecDesc(e.target.value)}
            placeholder="备注（可选）"
            className="rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-xs sm:col-span-3 dark:border-slate-700"
          />
          <button
            type="submit"
            className="inline-flex items-center justify-center rounded-lg bg-indigo-600 px-3 py-2 text-white hover:bg-indigo-700 sm:col-span-1"
          >
            <Plus className="h-4 w-4" />
          </button>
        </form>

        <div className="space-y-2">
          {secrets.map((s) => (
            <div
              key={s.id}
              className="flex items-center justify-between rounded-xl border border-slate-200 bg-white px-4 py-3 dark:border-slate-800 dark:bg-slate-900"
            >
              <div>
                <div className="flex items-center gap-2">
                  <span className="font-mono text-sm font-semibold text-slate-900 dark:text-slate-100">
                    {s.name}
                  </span>
                  <span className="rounded bg-slate-100 px-2 py-0.5 font-mono text-xs text-slate-500 dark:bg-slate-800 dark:text-slate-400">
                    {s.masked_value}
                  </span>
                </div>
                {s.description && <p className="mt-0.5 text-xs text-slate-500">{s.description}</p>}
              </div>
              <button
                onClick={async () => {
                  await api.deleteSecret(s.id);
                  await onRefresh();
                  showToast(`已删除 Secret ${s.name}`);
                }}
                className="rounded-lg p-1.5 text-slate-400 hover:bg-rose-50 hover:text-rose-600 dark:hover:bg-rose-950/40"
              >
                <Trash2 className="h-4 w-4" />
              </button>
            </div>
          ))}
        </div>
      </section>

      {/* Outbound Notifications */}
      <section className="space-y-3">
        <div>
          <h2 className="flex items-center gap-2 text-base font-bold text-slate-900 dark:text-slate-100">
            <Bell className="h-5 w-5 text-indigo-600 dark:text-indigo-400" />
            异步消息通知
          </h2>
          <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
            后台任务等待审批、完成或失败时，主动推送到飞书 / Slack / Discord / Webhook。
          </p>
        </div>

        <form
          onSubmit={handleAddNotification}
          className="grid gap-2.5 rounded-xl border border-slate-200 bg-white p-4 sm:grid-cols-12 dark:border-slate-800 dark:bg-slate-900"
        >
          <input
            value={notifName}
            onChange={(e) => setNotifName(e.target.value)}
            placeholder="渠道名称"
            className="rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-xs sm:col-span-3 dark:border-slate-700"
          />
          <select
            value={notifType}
            onChange={(e) => setNotifType(e.target.value as typeof notifType)}
            className="rounded-lg border border-slate-200 bg-transparent px-2.5 py-2 text-xs sm:col-span-2 dark:border-slate-700"
          >
            <option value="feishu">飞书</option>
            <option value="slack">Slack</option>
            <option value="discord">Discord</option>
            <option value="webhook">Webhook</option>
          </select>
          <input
            value={notifUrl}
            onChange={(e) => setNotifUrl(e.target.value)}
            placeholder="Webhook URL..."
            className="rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs sm:col-span-6 dark:border-slate-700"
          />
          <button
            type="submit"
            className="inline-flex items-center justify-center rounded-lg bg-indigo-600 px-3 py-2 text-white hover:bg-indigo-700 sm:col-span-1"
          >
            <Plus className="h-4 w-4" />
          </button>
        </form>

        <div className="space-y-2">
          {notifications.map((ch) => (
            <div
              key={ch.id}
              className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-slate-200 bg-white px-4 py-3 dark:border-slate-800 dark:bg-slate-900"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-slate-900 dark:text-slate-100">{ch.name}</span>
                  <span className="rounded bg-indigo-50 px-1.5 py-0.5 text-[11px] font-medium text-indigo-700 dark:bg-indigo-950/60 dark:text-indigo-300">
                    {ch.channel_type}
                  </span>
                </div>
                <p className="mt-0.5 truncate font-mono text-[11px] text-slate-400">
                  {ch.target_url}
                </p>
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={async () => {
                    try {
                      await api.testNotification(ch.id);
                      showToast('测试通知已发送');
                    } catch (err) {
                      showToast(`发送失败: ${err instanceof Error ? err.message : String(err)}`);
                    }
                  }}
                  className="inline-flex items-center gap-1 rounded-lg border border-slate-200 px-2.5 py-1 text-xs font-medium text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-800"
                >
                  <Send className="h-3 w-3" />
                  测试
                </button>
                <button
                  onClick={async () => {
                    await api.updateNotification(ch.id, { enabled: ch.enabled === 0 });
                    await onRefresh();
                  }}
                  className={`rounded-lg border px-2.5 py-1 text-xs font-medium ${
                    ch.enabled
                      ? 'border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-300'
                      : 'border-slate-200 bg-slate-50 text-slate-500 dark:border-slate-700 dark:bg-slate-800'
                  }`}
                >
                  {ch.enabled ? '已启用' : '已暂停'}
                </button>
                <button
                  onClick={async () => {
                    await api.deleteNotification(ch.id);
                    await onRefresh();
                    showToast('通知渠道已删除');
                  }}
                  className="rounded-lg p-1.5 text-slate-400 hover:bg-rose-50 hover:text-rose-600 dark:hover:bg-rose-950/40"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
