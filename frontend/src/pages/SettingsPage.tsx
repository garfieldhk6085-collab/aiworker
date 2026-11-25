import { useState } from 'react';
import { Bell, Cloud, Container, Cpu } from 'lucide-react';
import { BackupSyncCard } from '../components/settings/BackupSyncCard';
import { LlmSettingsCard } from '../components/settings/LlmSettingsCard';
import { RuntimeSettingsCard } from '../components/settings/RuntimeSettingsCard';
import { SecretsAndNotifyCard } from '../components/settings/SecretsAndNotifyCard';
import type {
  LlmSettings,
  NotificationChannel,
  RuntimeStatusResponse,
  SecretMeta,
} from '../types';

interface SettingsPageProps {
  llmSettings: LlmSettings;
  setLlmSettings: React.Dispatch<React.SetStateAction<LlmSettings>>;
  runtimeStatus: RuntimeStatusResponse | null;
  secrets: SecretMeta[];
  notifications: NotificationChannel[];
  onRefresh: () => Promise<void>;
  showToast: (msg: string) => void;
}

type SettingsTab = 'llm' | 'runtime' | 'security' | 'backup';

export function SettingsPage(props: SettingsPageProps) {
  const [subTab, setSubTab] = useState<SettingsTab>('llm');

  const tabs: Array<{ key: SettingsTab; label: string; icon: React.ReactNode }> = [
    { key: 'llm', label: '模型引擎', icon: <Cpu className="h-3.5 w-3.5" /> },
    { key: 'runtime', label: '沙箱与预算', icon: <Container className="h-3.5 w-3.5" /> },
    { key: 'security', label: '密钥与通知', icon: <Bell className="h-3.5 w-3.5" /> },
    { key: 'backup', label: '备份与 S3', icon: <Cloud className="h-3.5 w-3.5" /> },
  ];

  return (
    <div className="max-w-3xl space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight text-slate-900 dark:text-slate-100">
          模型与系统设置
        </h1>
        <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
          配置多模型路由、Python/Shell 沙箱边界、密钥保险箱、异步通知与 S3 云端同步。
        </p>
      </div>

      <div className="flex flex-wrap gap-1 rounded-xl border border-slate-200 bg-white p-1 text-xs font-medium dark:border-slate-800 dark:bg-slate-900">
        {tabs.map((t) => (
          <button
            key={t.key}
            onClick={() => setSubTab(t.key)}
            className={`inline-flex flex-1 items-center justify-center gap-1.5 rounded-lg px-3 py-2 whitespace-nowrap transition sm:flex-initial ${
              subTab === t.key
                ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-950/70 dark:text-indigo-300'
                : 'text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-200'
            }`}
          >
            {t.icon}
            {t.label}
          </button>
        ))}
      </div>

      {subTab === 'llm' && (
        <LlmSettingsCard
          llmSettings={props.llmSettings}
          setLlmSettings={props.setLlmSettings}
          onRefresh={props.onRefresh}
          showToast={props.showToast}
        />
      )}

      {subTab === 'runtime' && (
        <RuntimeSettingsCard
          runtimeStatus={props.runtimeStatus}
          onRefresh={props.onRefresh}
          showToast={props.showToast}
        />
      )}

      {subTab === 'security' && (
        <SecretsAndNotifyCard
          secrets={props.secrets}
          notifications={props.notifications}
          onRefresh={props.onRefresh}
          showToast={props.showToast}
        />
      )}

      {subTab === 'backup' && <BackupSyncCard showToast={props.showToast} />}
    </div>
  );
}
