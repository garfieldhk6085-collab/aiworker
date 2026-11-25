import { CheckCircle2 } from 'lucide-react';
import { api } from '../../api';
import type { LlmSettings } from '../../types';

interface Props {
  llmSettings: LlmSettings;
  setLlmSettings: React.Dispatch<React.SetStateAction<LlmSettings>>;
  onRefresh: () => Promise<void>;
  showToast: (msg: string) => void;
}

export function LlmSettingsCard({
  llmSettings,
  setLlmSettings,
  onRefresh,
  showToast,
}: Props) {
  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    await api.saveLlmSettings(llmSettings);
    await onRefresh();
    showToast('模型配置已保存');
  };

  const presets: Array<{ label: string; patch: Partial<LlmSettings> }> = [
    { label: '内置 Mock', patch: { provider: 'mock' } },
    { label: 'OpenAI', patch: { provider: 'openai', base_url: '', model: 'gpt-4o-mini' } },
    {
      label: 'Claude',
      patch: { provider: 'anthropic', base_url: '', model: 'claude-3-7-sonnet-latest' },
    },
    { label: 'Gemini', patch: { provider: 'gemini', base_url: '', model: 'gemini-2.5-flash' } },
    {
      label: 'DeepSeek',
      patch: {
        provider: 'openai_compatible',
        base_url: 'https://api.deepseek.com/v1',
        model: 'deepseek-chat',
      },
    },
    {
      label: 'Ollama',
      patch: {
        provider: 'ollama',
        base_url: 'http://127.0.0.1:11434',
        api_key: '',
        model: 'qwen2.5:7b',
      },
    },
  ];

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap gap-1.5">
        {presets.map((p) => (
          <button
            key={p.label}
            type="button"
            onClick={() => setLlmSettings((s) => ({ ...s, ...p.patch }))}
            className="rounded-lg border border-slate-200 bg-white px-2.5 py-1.5 text-xs font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200 dark:hover:bg-slate-800"
          >
            {p.label}
          </button>
        ))}
      </div>

      <form
        onSubmit={handleSave}
        className="space-y-4 rounded-2xl border border-slate-200 bg-white p-5 sm:p-6 dark:border-slate-800 dark:bg-slate-900"
      >
        <div>
          <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
            模型提供商 (rig-core)
          </label>
          <select
            value={llmSettings.provider}
            onChange={(e) =>
              setLlmSettings({
                ...llmSettings,
                provider: e.target.value as LlmSettings['provider'],
              })
            }
            className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-sm dark:border-slate-700"
          >
            <option value="mock">Mock 演示模式（无需 API Key）</option>
            <option value="openai">OpenAI</option>
            <option value="anthropic">Anthropic (Claude)</option>
            <option value="gemini">Google Gemini</option>
            <option value="ollama">Ollama（本地模型）</option>
            <option value="openai_compatible">自定义 OpenAI-Compatible API</option>
          </select>
        </div>

        {llmSettings.provider !== 'mock' && (
          <>
            <div>
              <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
                Base URL（留空则使用官方默认地址）
              </label>
              <input
                type="text"
                value={llmSettings.base_url}
                onChange={(e) => setLlmSettings({ ...llmSettings, base_url: e.target.value })}
                placeholder="可选，如自定义代理或 DeepSeek / Ollama 地址"
                className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-sm dark:border-slate-700"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
                API Key（保存后自动脱敏，本地 Ollama 可留空）
              </label>
              <input
                type="password"
                value={llmSettings.api_key}
                onChange={(e) => setLlmSettings({ ...llmSettings, api_key: e.target.value })}
                placeholder="sk-..."
                className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-sm dark:border-slate-700"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
                Model 名称
              </label>
              <input
                type="text"
                value={llmSettings.model}
                onChange={(e) => setLlmSettings({ ...llmSettings, model: e.target.value })}
                className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 font-mono text-sm dark:border-slate-700"
              />
            </div>
          </>
        )}

        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              Temperature
            </label>
            <input
              type="number"
              step="0.1"
              min="0"
              max="2"
              value={llmSettings.temperature}
              onChange={(e) =>
                setLlmSettings({ ...llmSettings, temperature: parseFloat(e.target.value) || 0.2 })
              }
              className="w-full rounded-lg border border-slate-200 bg-transparent px-3 py-2 text-sm dark:border-slate-700"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-400">
              单任务最大步数
            </label>
            <input
              type="number"
              min="2"
              max="30"
              value={llmSettings.default_max_steps}
              onChange={(e) =>
                setLlmSettings({
                  ...llmSettings,
                  default_max_steps: parseInt(e.target.value, 10) || 12,
                })
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
          保存模型配置
        </button>
      </form>
    </section>
  );
}
