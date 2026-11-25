import { useCallback, useEffect, useState } from 'react';
import {
  ArrowLeft,
  Download,
  FileText,
  Folder,
  Plus,
  RefreshCw,
  Trash2,
  Upload,
} from 'lucide-react';
import { api } from '../api';
import type { WorkspaceEntry } from '../types';

interface WorkspacePageProps {
  showToast: (msg: string) => void;
}

export function WorkspacePage({ showToast }: WorkspacePageProps) {
  const [currentPath, setCurrentPath] = useState('');
  const [entries, setEntries] = useState<WorkspaceEntry[]>([]);
  const [loading, setLoading] = useState(false);

  const [newPath, setNewPath] = useState('');
  const [newContent, setNewContent] = useState('');
  const [showEditor, setShowEditor] = useState(false);

  const loadFiles = useCallback(async (path: string) => {
    setLoading(true);
    try {
      const list = await api.listWorkspaceFiles(path);
      setEntries(list);
    } catch (err) {
      showToast(`加载工作区失败: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setLoading(false);
    }
  }, [showToast]);

  useEffect(() => {
    loadFiles(currentPath);
  }, [currentPath, loadFiles]);

  const handleParent = () => {
    const parts = currentPath.split('/').filter(Boolean);
    parts.pop();
    setCurrentPath(parts.join('/'));
  };

  const handleFileUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const base64 = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => {
          const result = String(reader.result ?? '');
          const commaIdx = result.indexOf(',');
          resolve(commaIdx >= 0 ? result.slice(commaIdx + 1) : result);
        };
        reader.onerror = () => reject(new Error('读取文件失败'));
        reader.readAsDataURL(file);
      });
      const targetPath = currentPath ? `${currentPath}/${file.name}` : file.name;
      await api.uploadWorkspaceFile(targetPath, base64, true);
      await loadFiles(currentPath);
      showToast(`已上传 ${targetPath}`);
    } catch (err) {
      showToast(`上传失败: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      e.target.value = '';
    }
  };

  const handleSaveNewFile = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newPath.trim()) return;
    try {
      const targetPath = currentPath && !newPath.includes('/')
        ? `${currentPath}/${newPath.trim()}`
        : newPath.trim();
      await api.uploadWorkspaceFile(targetPath, newContent);
      setNewPath('');
      setNewContent('');
      setShowEditor(false);
      await loadFiles(currentPath);
      showToast(`已保存 ${targetPath}`);
    } catch (err) {
      showToast(`保存失败: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  const formatBytes = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight text-slate-900">工作区文件</h1>
          <p className="mt-1 text-sm text-slate-500">
            上传 CSV、笔记或资料供 AI Worker 处理，并下载 AI 在沙箱中生成的报告和产物。
          </p>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={() => setShowEditor((v) => !v)}
            className="inline-flex items-center gap-1.5 rounded-xl border border-slate-200 bg-white px-3 py-2 text-xs font-medium text-slate-700 hover:bg-slate-50"
          >
            <Plus className="h-4 w-4" />
            新建文本/笔记
          </button>

          <label className="inline-flex cursor-pointer items-center gap-1.5 rounded-xl bg-indigo-600 px-3.5 py-2 text-xs font-medium text-white shadow-xs hover:bg-indigo-700">
            <Upload className="h-4 w-4" />
            上传文件
            <input type="file" className="hidden" onChange={handleFileUpload} />
          </label>
        </div>
      </div>

      {showEditor && (
        <form
          onSubmit={handleSaveNewFile}
          className="space-y-3 rounded-2xl border border-slate-200 bg-white p-4 shadow-xs"
        >
          <input
            value={newPath}
            onChange={(e) => setNewPath(e.target.value)}
            placeholder="相对路径，例如 notes/todo.md 或 data/input.csv"
            className="w-full rounded-lg border border-slate-200 px-3 py-2 font-mono text-sm"
          />
          <textarea
            rows={5}
            value={newContent}
            onChange={(e) => setNewContent(e.target.value)}
            placeholder="输入文件内容..."
            className="w-full rounded-lg border border-slate-200 px-3 py-2 font-mono text-xs"
          />
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setShowEditor(false)}
              className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs font-medium text-slate-600"
            >
              取消
            </button>
            <button
              type="submit"
              className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-700"
            >
              保存到工作区
            </button>
          </div>
        </form>
      )}

      <div className="overflow-hidden rounded-2xl border border-slate-200 bg-white">
        <div className="flex items-center justify-between border-b border-slate-100 bg-slate-50 px-4 py-2.5 text-xs">
          <div className="flex items-center gap-2 font-mono text-slate-600">
            {currentPath && (
              <button
                onClick={handleParent}
                className="inline-flex items-center gap-1 rounded bg-white px-2 py-1 text-slate-700 shadow-2xs hover:bg-slate-100"
              >
                <ArrowLeft className="h-3.5 w-3.5" />
                上级
              </button>
            )}
            <span>workspace/{currentPath}</span>
          </div>
          <button
            onClick={() => loadFiles(currentPath)}
            className="rounded p-1 text-slate-500 hover:bg-slate-200/70"
            title="刷新"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
          </button>
        </div>

        {entries.length === 0 ? (
          <div className="p-10 text-center text-sm text-slate-400">
            当前目录为空。你可以上传文件或让 AI Worker 生成报告。
          </div>
        ) : (
          <div className="divide-y divide-slate-100">
            {entries.map((entry) => (
              <div
                key={entry.path}
                className="flex items-center justify-between gap-4 px-4 py-3 hover:bg-slate-50"
              >
                <button
                  onClick={() => {
                    if (entry.is_dir) {
                      setCurrentPath(entry.path);
                    }
                  }}
                  className={`flex min-w-0 flex-1 items-center gap-2.5 text-left ${
                    entry.is_dir ? 'cursor-pointer' : 'cursor-default'
                  }`}
                >
                  {entry.is_dir ? (
                    <Folder className="h-4 w-4 shrink-0 text-amber-500" />
                  ) : (
                    <FileText className="h-4 w-4 shrink-0 text-slate-400" />
                  )}
                  <span className="truncate font-mono text-sm text-slate-800">{entry.name}</span>
                </button>

                <div className="flex items-center gap-3">
                  {!entry.is_dir && (
                    <span className="text-xs text-slate-400">{formatBytes(entry.size_bytes)}</span>
                  )}
                  {!entry.is_dir && (
                    <a
                      href={api.downloadWorkspaceUrl(entry.path)}
                      download={entry.name}
                      className="rounded-lg p-1.5 text-slate-500 hover:bg-indigo-50 hover:text-indigo-600"
                      title="下载"
                    >
                      <Download className="h-4 w-4" />
                    </a>
                  )}
                  <button
                    onClick={async () => {
                      await api.deleteWorkspaceFile(entry.path);
                      await loadFiles(currentPath);
                      showToast(`已删除 ${entry.path}`);
                    }}
                    className="rounded-lg p-1.5 text-slate-400 hover:bg-rose-50 hover:text-rose-600"
                    title="删除"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
