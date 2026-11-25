import { useEffect, useState } from 'react';
import {
  CheckCircle2,
  Cloud,
  Download,
  RefreshCw,
  RotateCcw,
  UploadCloud,
} from 'lucide-react';
import { api } from '../../api';
import type { BackupSettings, S3BackupObject } from '../../types';

interface Props {
  showToast: (msg: string) => void;
}

export function BackupSyncCard({ showToast }: Props) {
  const [form, setForm] = useState<BackupSettings>({
    enabled: false,
    endpoint: '',
    region: 'us-east-1',
    bucket: '',
    prefix: 'aiworker/',
    access_key_id: '',
    secret_access_key: '',
    force_path_style: true,
    auto_backup_cron: '0 3 * * *',
    last_backup_at: null,
    last_backup_key: null,
  });
  const [s3Objects, setS3Objects] = useState<S3BackupObject[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .getBackupSettings()
      .then((cfg) => setForm(cfg))
      .catch(() => {});
  }, []);

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      const saved = await api.saveBackupSettings(form);
      setForm(saved);
      showToast('备份与 S3 配置已保存');
    } catch (err) {
      showToast(`保存失败: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  const handlePush = async () => {
    setBusy(true);
    try {
      const res = await api.pushS3Backup();
      setForm(await api.getBackupSettings());
      showToast(`已推送至 S3: ${res.key}`);
      setS3Objects(await api.listS3Backups().catch(() => []));
    } catch (err) {
      showToast(`S3 推送失败: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleList = async () => {
    setBusy(true);
    try {
      const list = await api.listS3Backups();
      setS3Objects(list);
      showToast(`已获取 ${list.length} 个 S3 历史备份`);
    } catch (err) {
      showToast(`拉取列表失败: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleRestore = async (key: string) => {
    if (!window.confirm(`确认从 S3 备份 "${key}" 恢复工作区文件吗？`)) return;
    setBusy(true);
    try {
      const res = await api.restoreS3Backup(key);
      showToast(
        `已恢复 ${res.restored_files} 个文件${res.db_snapshot_saved_to ? '，DB 快照已另存为 aiworker.db.restored' : ''}`,
      );
    } catch (err) {
      showToast(`恢复失败: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="flex items-center gap-2 text-base font-bold text-slate-900 dark:text-slate-100">
            <Cloud className="h-5 w-5 text-indigo-600 dark:text-indigo-400" />
            备份与 S3 同步
          </h2>
          <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
            SQLite 热备快照 + workspace 全量打包，兼容 AWS S3 / Cloudflare R2 / MinIO / 阿里云 OSS。
          </p>
        </div>
        <a
          href={api.exportLocalBackupUrl()}
          className="inline-flex items-center gap-1.5 rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200"
        >
          <Download className="h-4 w-4 text-indigo-600 dark:text-indigo-400" />
          下载本地备份 (.tar.gz)
        </a>
      </div>

      <form
        onSubmit={handleSave}
        className="space-y-4 rounded-2xl border border-slate-200 bg-white p-5 sm:p-6 dark:border-slate-800 dark:bg-slate-900"
      >
        <div className="flex flex-wrap items-center justify-between gap-2">
          <label className="inline-flex items-center gap-2 text-xs font-medium text-slate-700 dark:text-slate-200">
            <input
              type="checkbox"
              checked={form.enabled}
              onChange={(e) => setForm({ ...form, enabled: e.target.checked })}
              className="h-4 w-4 rounded border-slate-300 text-indigo-600"
            />
            开启定时自动备份到 S3
          </label>
          <div className="flex flex-wrap gap-1.5 text-[11px]">
            <button
              type="button"
              onClick={() =>
                setForm((b) => ({ ...b, endpoint: '', region: 'us-east-1', force_path_style: false }))
              }
              className="rounded-md border border-slate-200 px-2 py-1 text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300"
            >
              AWS S3
            </button>
            <button
              type="button"
              onClick={() =>
                setForm((b) => ({
                  ...b,
                  endpoint: 'https://<account_id>.r2.cloudflarestorage.com',
                  region: 'auto',
                  force_path_style: true,
                }))
              }
              className="rounded-md border border-slate-200 px-2 py-1 text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300"
            >
              Cloudflare R2
            </button>
            <button
              type="button"
              onClick={() =>
                setForm((b) => ({
                  ...b,
                  endpoint: 'http://127.0.0.1:9000',
                  region: 'us-east-1',
                  force_path_style: true,
                }))
              }
              className="rounded-md border border-slate-200 px-2 py-1 text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300"
            >
              MinIO
            </button>
          </div>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              Endpoint URL（AWS 可留空）
            </label>
            <input
              value={form.endpoint}
              onChange={(e) => setForm({ ...form, endpoint: e.target.value })}
              placeholder="https://... 或 http://127.0.0.1:9000"
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs dark:border-slate-700"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              Bucket 名称
            </label>
            <input
              value={form.bucket}
              onChange={(e) => setForm({ ...form, bucket: e.target.value })}
              placeholder="my-aiworker-backups"
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs dark:border-slate-700"
            />
          </div>
        </div>

        <div className="grid gap-4 sm:grid-cols-3">
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              Region
            </label>
            <input
              value={form.region}
              onChange={(e) => setForm({ ...form, region: e.target.value })}
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs dark:border-slate-700"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              路径前缀
            </label>
            <input
              value={form.prefix}
              onChange={(e) => setForm({ ...form, prefix: e.target.value })}
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs dark:border-slate-700"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              定时 Cron
            </label>
            <input
              value={form.auto_backup_cron}
              onChange={(e) => setForm({ ...form, auto_backup_cron: e.target.value })}
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs dark:border-slate-700"
            />
          </div>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              Access Key ID
            </label>
            <input
              value={form.access_key_id}
              onChange={(e) => setForm({ ...form, access_key_id: e.target.value })}
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs dark:border-slate-700"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              Secret Access Key
            </label>
            <input
              type="password"
              value={form.secret_access_key}
              onChange={(e) => setForm({ ...form, secret_access_key: e.target.value })}
              placeholder="保存后自动脱敏"
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-xs dark:border-slate-700"
            />
          </div>
        </div>

        <label className="inline-flex items-center gap-2 text-xs text-slate-600 dark:text-slate-400">
          <input
            type="checkbox"
            checked={form.force_path_style}
            onChange={(e) => setForm({ ...form, force_path_style: e.target.checked })}
            className="h-4 w-4 rounded border-slate-300 text-indigo-600"
          />
          使用 Path-Style URL（MinIO / Cloudflare R2 推荐）
        </label>

        {form.last_backup_at && (
          <div className="rounded-lg bg-slate-50 px-3 py-2 text-xs text-slate-500 dark:bg-slate-800/70 dark:text-slate-400">
            上次同步：{new Date(form.last_backup_at).toLocaleString()} ·{' '}
            <span className="font-mono text-slate-700 dark:text-slate-200">
              {form.last_backup_key}
            </span>
          </div>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <button
            type="submit"
            className="inline-flex items-center gap-1.5 rounded-xl bg-indigo-600 px-4 py-2 text-xs font-medium text-white hover:bg-indigo-700"
          >
            <CheckCircle2 className="h-4 w-4" />
            保存配置
          </button>
          <button
            type="button"
            disabled={busy || !form.bucket}
            onClick={handlePush}
            className="inline-flex items-center gap-1.5 rounded-xl border border-emerald-200 bg-emerald-50 px-3.5 py-2 text-xs font-medium text-emerald-700 hover:bg-emerald-100 disabled:opacity-50 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-300"
          >
            <UploadCloud className="h-4 w-4" />
            立即推送 S3
          </button>
          <button
            type="button"
            disabled={busy || !form.bucket}
            onClick={handleList}
            className="inline-flex items-center gap-1.5 rounded-xl border border-slate-200 bg-white px-3.5 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${busy ? 'animate-spin' : ''}`} />
            拉取历史
          </button>
        </div>
      </form>

      {s3Objects.length > 0 && (
        <div className="divide-y divide-slate-100 overflow-hidden rounded-xl border border-slate-200 bg-white dark:divide-slate-800 dark:border-slate-800 dark:bg-slate-900">
          {s3Objects.map((obj) => (
            <div
              key={obj.key}
              className="flex flex-wrap items-center justify-between gap-2 px-4 py-3 text-xs"
            >
              <div className="min-w-0">
                <div className="truncate font-mono font-medium text-slate-800 dark:text-slate-200">
                  {obj.key}
                </div>
                <div className="mt-0.5 text-slate-400">
                  {(obj.size_bytes / 1024).toFixed(1)} KB · {obj.last_modified}
                </div>
              </div>
              <button
                disabled={busy}
                onClick={() => handleRestore(obj.key)}
                className="inline-flex shrink-0 items-center gap-1 rounded-lg border border-indigo-200 bg-indigo-50 px-2.5 py-1.5 font-medium text-indigo-700 hover:bg-indigo-100 disabled:opacity-50 dark:border-indigo-900 dark:bg-indigo-950/50 dark:text-indigo-300"
              >
                <RotateCcw className="h-3.5 w-3.5" />
                恢复
              </button>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
