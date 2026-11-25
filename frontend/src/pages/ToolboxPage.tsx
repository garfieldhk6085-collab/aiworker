import { useState } from 'react';
import { BookOpen, Bot, Brain, Plus, Trash2, Wrench } from 'lucide-react';
import { api } from '../api';
import type { MemoryItem, SkillSummary, ToolInfo, ToolPolicy, Worker } from '../types';

interface ToolboxPageProps {
  tools: ToolInfo[];
  skills: SkillSummary[];
  workers: Worker[];
  memories: MemoryItem[];
  onRefresh: () => Promise<void>;
  showToast: (msg: string) => void;
}

type SubTab = 'tools' | 'skills' | 'workers' | 'memory';

export function ToolboxPage({
  tools,
  skills,
  workers,
  memories,
  onRefresh,
  showToast,
}: ToolboxPageProps) {
  const [subTab, setSubTab] = useState<SubTab>('tools');

  // Skill form
  const [skillName, setSkillName] = useState('');
  const [skillDesc, setSkillDesc] = useState('');
  const [skillContent, setSkillContent] = useState('');
  const [editingSkill, setEditingSkill] = useState(false);

  // Worker form
  const [workerName, setWorkerName] = useState('');
  const [workerDesc, setWorkerDesc] = useState('');
  const [workerPrompt, setWorkerPrompt] = useState('');
  const [workerProvider, setWorkerProvider] = useState('');
  const [workerModel, setWorkerModel] = useState('');
  const [workerAllowedTools, setWorkerAllowedTools] = useState<string[]>([]);

  // Memory form
  const [memKey, setMemKey] = useState('');
  const [memContent, setMemContent] = useState('');
  const [memCategory, setMemCategory] = useState('preference');

  const handlePolicyChange = async (name: string, policy: ToolPolicy) => {
    await api.updateToolPolicy(name, policy);
    await onRefresh();
    showToast(`已更新工具 ${name} 权限策略`);
  };

  const handleSaveSkill = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!skillName.trim() || !skillContent.trim()) return;
    try {
      await api.upsertSkill({
        name: skillName.trim(),
        description: skillDesc.trim() || '自定义技能',
        content: skillContent,
      });
      setSkillName('');
      setSkillDesc('');
      setSkillContent('');
      setEditingSkill(false);
      await onRefresh();
      showToast('Skill 已保存');
    } catch (err) {
      showToast(`保存失败: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  const handleEditSkill = async (name: string) => {
    try {
      const detail = await api.getSkill(name);
      setSkillName(detail.name);
      setSkillDesc(detail.description);
      setSkillContent(detail.content);
      setEditingSkill(true);
    } catch (err) {
      showToast(`加载 Skill 失败: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  const handleCreateWorker = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!workerName.trim() || !workerPrompt.trim()) return;
    try {
      await api.createWorker({
        name: workerName.trim(),
        description: workerDesc.trim(),
        system_prompt: workerPrompt.trim(),
        provider: workerProvider || undefined,
        model: workerModel.trim() || undefined,
        allowed_tools: workerAllowedTools.length > 0 ? workerAllowedTools : undefined,
      });
      setWorkerName('');
      setWorkerDesc('');
      setWorkerPrompt('');
      setWorkerProvider('');
      setWorkerModel('');
      setWorkerAllowedTools([]);
      await onRefresh();
      showToast('Worker 角色预设已创建');
    } catch (err) {
      showToast(`创建失败: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  const handleAddMemory = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!memKey.trim() || !memContent.trim()) return;
    await api.upsertMemory({
      key: memKey.trim(),
      content: memContent.trim(),
      category: memCategory,
    });
    setMemKey('');
    setMemContent('');
    await onRefresh();
    showToast('已保存个人记忆');
  };

  const toggleWorkerTool = (name: string) => {
    setWorkerAllowedTools((prev) =>
      prev.includes(name) ? prev.filter((x) => x !== name) : [...prev, name],
    );
  };

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight text-slate-900">能力中心</h1>
        <p className="mt-1 text-sm text-slate-500">
          统一管理工具权限边界、Skills 技能包（SKILL.md）、Worker 角色预设与个人记忆库。
        </p>
      </div>

      {/* Sub-navigation */}
      <div className="inline-flex rounded-xl border border-slate-200 bg-white p-1 text-xs font-medium">
        {(
          [
            ['tools', '内置工具', Wrench, tools.length],
            ['skills', 'Skills 技能', BookOpen, skills.length],
            ['workers', 'Worker 角色', Bot, workers.length],
            ['memory', '个人记忆', Brain, memories.length],
          ] as const
        ).map(([key, label, Icon, count]) => (
          <button
            key={key}
            onClick={() => setSubTab(key)}
            className={`inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 transition ${
              subTab === key
                ? 'bg-indigo-50 text-indigo-700'
                : 'text-slate-600 hover:text-slate-900'
            }`}
          >
            <Icon className="h-3.5 w-3.5" />
            {label}
            <span className="rounded bg-slate-100 px-1.5 py-0.2 text-[10px] text-slate-500">
              {count}
            </span>
          </button>
        ))}
      </div>

      {/* Tools */}
      {subTab === 'tools' && (
        <div className="grid gap-3 sm:grid-cols-2">
          {tools.map((tool) => (
            <div
              key={tool.name}
              className="flex flex-col justify-between rounded-xl border border-slate-200 bg-white p-4"
            >
              <div>
                <div className="flex items-center justify-between">
                  <span className="font-mono text-sm font-semibold text-slate-900">
                    {tool.name}
                  </span>
                  <span
                    className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${
                      tool.risk_level === 'read_only'
                        ? 'bg-emerald-50 text-emerald-700'
                        : tool.risk_level === 'exec'
                          ? 'bg-rose-50 text-rose-700'
                          : 'bg-amber-50 text-amber-700'
                    }`}
                  >
                    {tool.risk_level}
                  </span>
                </div>
                <p className="mt-1.5 text-xs leading-relaxed text-slate-500">
                  {tool.description}
                </p>
              </div>

              <div className="mt-4 flex rounded-lg bg-slate-100 p-1 text-xs">
                {(
                  [
                    ['auto_allow', '自动允许'],
                    ['require_approval', '需审批'],
                    ['disabled', '禁用'],
                  ] as const
                ).map(([policy, label]) => (
                  <button
                    key={policy}
                    onClick={() => handlePolicyChange(tool.name, policy)}
                    className={`flex-1 rounded-md py-1 font-medium transition ${
                      tool.effective_policy === policy
                        ? 'bg-white text-slate-900 shadow-2xs'
                        : 'text-slate-500 hover:text-slate-800'
                    }`}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Skills */}
      {subTab === 'skills' && (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <p className="text-xs text-slate-500">
              技能以 <span className="font-mono">workspace/skills/&lt;name&gt;/SKILL.md</span> 存储，按需加载以节省 Token。
            </p>
            <button
              onClick={() => {
                setSkillName('');
                setSkillDesc('');
                setSkillContent('');
                setEditingSkill((v) => !v);
              }}
              className="inline-flex items-center gap-1 rounded-xl bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-700"
            >
              <Plus className="h-3.5 w-3.5" />
              新建 Skill
            </button>
          </div>

          {editingSkill && (
            <form
              onSubmit={handleSaveSkill}
              className="space-y-3 rounded-2xl border border-slate-200 bg-white p-4 shadow-xs"
            >
              <div className="grid gap-3 sm:grid-cols-2">
                <input
                  value={skillName}
                  onChange={(e) => setSkillName(e.target.value)}
                  placeholder="技能标识（英文/数字/-/_，如 csv_analyzer）"
                  className="rounded-lg border border-slate-200 px-3 py-2 font-mono text-sm"
                />
                <input
                  value={skillDesc}
                  onChange={(e) => setSkillDesc(e.target.value)}
                  placeholder="一句话描述（AI 据此决定是否加载该技能）"
                  className="rounded-lg border border-slate-200 px-3 py-2 text-sm"
                />
              </div>
              <textarea
                rows={6}
                value={skillContent}
                onChange={(e) => setSkillContent(e.target.value)}
                placeholder="填写 Markdown 操作指南、Python/Shell 命令示例或注意事项..."
                className="w-full rounded-lg border border-slate-200 px-3 py-2 font-mono text-xs"
              />
              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setEditingSkill(false)}
                  className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs font-medium text-slate-600"
                >
                  取消
                </button>
                <button
                  type="submit"
                  className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-700"
                >
                  保存 Skill
                </button>
              </div>
            </form>
          )}

          {skills.length === 0 ? (
            <div className="rounded-xl border border-dashed border-slate-200 bg-white p-8 text-center text-sm text-slate-400">
              尚未安装任何 Skill。点击右上角「新建 Skill」添加你的第一个专属技能包。
            </div>
          ) : (
            <div className="grid gap-3 sm:grid-cols-2">
              {skills.map((s) => (
                <div
                  key={s.name}
                  className="flex flex-col justify-between rounded-xl border border-slate-200 bg-white p-4"
                >
                  <div>
                    <div className="flex items-center justify-between">
                      <span className="font-mono text-sm font-semibold text-slate-900">
                        {s.name}
                      </span>
                      <div className="flex items-center gap-1">
                        <button
                          onClick={() => handleEditSkill(s.name)}
                          className="rounded px-2 py-1 text-xs font-medium text-indigo-600 hover:bg-indigo-50"
                        >
                          编辑
                        </button>
                        <button
                          onClick={async () => {
                            await api.deleteSkill(s.name);
                            await onRefresh();
                            showToast(`已删除 Skill ${s.name}`);
                          }}
                          className="rounded p-1 text-slate-400 hover:bg-rose-50 hover:text-rose-600"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    </div>
                    <p className="mt-1 text-xs text-slate-500">{s.description}</p>
                    {s.files.length > 0 && (
                      <div className="mt-2 flex flex-wrap gap-1">
                        {s.files.map((f) => (
                          <span
                            key={f}
                            className="rounded bg-slate-100 px-1.5 py-0.5 font-mono text-[10px] text-slate-600"
                          >
                            {f}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Workers */}
      {subTab === 'workers' && (
        <div className="space-y-4">
          <form
            onSubmit={handleCreateWorker}
            className="space-y-3 rounded-2xl border border-slate-200 bg-white p-4 shadow-xs"
          >
            <div className="text-xs font-semibold text-slate-700">新建 Worker 角色预设</div>
            <div className="grid gap-3 sm:grid-cols-2">
              <input
                value={workerName}
                onChange={(e) => setWorkerName(e.target.value)}
                placeholder="角色名称（如：数据分析师 / 每日早报官）"
                className="rounded-lg border border-slate-200 px-3 py-2 text-sm"
              />
              <input
                value={workerDesc}
                onChange={(e) => setWorkerDesc(e.target.value)}
                placeholder="简短说明（可选）"
                className="rounded-lg border border-slate-200 px-3 py-2 text-sm"
              />
            </div>
            <textarea
              rows={3}
              value={workerPrompt}
              onChange={(e) => setWorkerPrompt(e.target.value)}
              placeholder="专属 System Prompt（例如：你是一位严谨的数据分析师，优先使用 python_exec 清洗 CSV 并输出结论...）"
              className="w-full rounded-lg border border-slate-200 px-3 py-2 text-sm"
            />
            <div className="grid gap-3 sm:grid-cols-2">
              <select
                value={workerProvider}
                onChange={(e) => setWorkerProvider(e.target.value)}
                className="rounded-lg border border-slate-200 px-3 py-2 text-xs"
              >
                <option value="">继承全局默认 Provider</option>
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic (Claude)</option>
                <option value="gemini">Google Gemini</option>
                <option value="ollama">Ollama</option>
                <option value="openai_compatible">OpenAI-Compatible</option>
                <option value="mock">Mock</option>
              </select>
              <input
                value={workerModel}
                onChange={(e) => setWorkerModel(e.target.value)}
                placeholder="可选覆盖模型名（留空则继承全局）"
                className="rounded-lg border border-slate-200 px-3 py-2 font-mono text-xs"
              />
            </div>
            <div>
              <div className="mb-1.5 text-xs text-slate-500">
                允许使用的工具（不勾选则允许所有未禁用工具）：
              </div>
              <div className="flex flex-wrap gap-1.5">
                {tools.map((t) => {
                  const selected = workerAllowedTools.includes(t.name);
                  return (
                    <button
                      type="button"
                      key={t.name}
                      onClick={() => toggleWorkerTool(t.name)}
                      className={`rounded-lg border px-2.5 py-1 font-mono text-xs transition ${
                        selected
                          ? 'border-indigo-300 bg-indigo-50 text-indigo-700'
                          : 'border-slate-200 bg-slate-50 text-slate-600'
                      }`}
                    >
                      {t.name}
                    </button>
                  );
                })}
              </div>
            </div>
            <div className="flex justify-end">
              <button
                type="submit"
                className="inline-flex items-center gap-1 rounded-xl bg-indigo-600 px-4 py-2 text-xs font-medium text-white hover:bg-indigo-700"
              >
                <Plus className="h-3.5 w-3.5" />
                创建 Worker
              </button>
            </div>
          </form>

          <div className="space-y-2">
            {workers.map((w) => (
              <div
                key={w.id}
                className="flex items-start justify-between gap-3 rounded-xl border border-slate-200 bg-white p-4"
              >
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-semibold text-slate-900">{w.name}</span>
                    {w.provider && (
                      <span className="rounded bg-indigo-50 px-1.5 py-0.5 font-mono text-[11px] text-indigo-700">
                        {w.provider}
                        {w.model ? ` / ${w.model}` : ''}
                      </span>
                    )}
                  </div>
                  {w.description && <p className="mt-0.5 text-xs text-slate-500">{w.description}</p>}
                  <p className="mt-1.5 line-clamp-2 text-xs text-slate-600">{w.system_prompt}</p>
                </div>
                <button
                  onClick={async () => {
                    await api.deleteWorker(w.id);
                    await onRefresh();
                    showToast(`已删除 Worker ${w.name}`);
                  }}
                  className="rounded-lg p-1.5 text-slate-400 hover:bg-rose-50 hover:text-rose-600"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Memory */}
      {subTab === 'memory' && (
        <div className="space-y-4">
          <form
            onSubmit={handleAddMemory}
            className="grid gap-3 rounded-xl border border-slate-200 bg-white p-4 sm:grid-cols-12"
          >
            <input
              value={memKey}
              onChange={(e) => setMemKey(e.target.value)}
              placeholder="键名（如：输出风格）"
              className="rounded-lg border border-slate-200 px-3 py-2 text-sm sm:col-span-3"
            />
            <input
              value={memContent}
              onChange={(e) => setMemContent(e.target.value)}
              placeholder="内容（如：所有周报和摘要请用中文要点列出）"
              className="rounded-lg border border-slate-200 px-3 py-2 text-sm sm:col-span-6"
            />
            <select
              value={memCategory}
              onChange={(e) => setMemCategory(e.target.value)}
              className="rounded-lg border border-slate-200 px-2.5 py-2 text-sm sm:col-span-2"
            >
              <option value="preference">偏好</option>
              <option value="project">项目</option>
              <option value="profile">个人</option>
            </select>
            <button
              type="submit"
              className="inline-flex items-center justify-center rounded-lg bg-indigo-600 px-3 py-2 text-sm font-medium text-white hover:bg-indigo-700 sm:col-span-1"
            >
              <Plus className="h-4 w-4" />
            </button>
          </form>

          <div className="space-y-2">
            {memories.map((m) => (
              <div
                key={m.id}
                className="flex items-center justify-between gap-3 rounded-xl border border-slate-200 bg-white px-4 py-3"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="rounded bg-indigo-50 px-1.5 py-0.5 text-[11px] font-medium text-indigo-700">
                      {m.category}
                    </span>
                    <span className="font-medium text-slate-900">{m.key}</span>
                  </div>
                  <p className="mt-1 text-xs text-slate-600">{m.content}</p>
                </div>
                <button
                  onClick={async () => {
                    await api.deleteMemory(m.id);
                    await onRefresh();
                  }}
                  className="rounded-lg p-1.5 text-slate-400 hover:bg-rose-50 hover:text-rose-600"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
