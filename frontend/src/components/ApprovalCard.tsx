import { useState } from 'react';
import { Check, ShieldAlert, X } from 'lucide-react';
import type { ApprovalWithContext } from '../types';

interface ApprovalCardProps {
  item: ApprovalWithContext;
  onDecide: (id: string, decision: 'approved' | 'rejected', feedback?: string) => Promise<void>;
  onOpenRun: (runId: string) => void;
}

export function ApprovalCard({ item, onDecide, onOpenRun }: ApprovalCardProps) {
  const [feedback, setFeedback] = useState('');
  const [submitting, setSubmitting] = useState(false);

  let parsedArgs: Record<string, unknown> | null = null;
  try {
    parsedArgs = JSON.parse(item.arguments_json);
  } catch {
    parsedArgs = null;
  }

  const handleAction = async (decision: 'approved' | 'rejected') => {
    setSubmitting(true);
    try {
      await onDecide(item.id, decision, feedback.trim() || undefined);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="rounded-xl border border-amber-200 bg-white p-4 shadow-xs transition hover:border-amber-300">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-amber-100 text-amber-700">
            <ShieldAlert className="h-4 w-4" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <span className="font-semibold text-slate-900">{item.task_title}</span>
              <span className="rounded bg-slate-100 px-1.5 py-0.5 font-mono text-xs text-slate-700">
                {item.tool_name}
              </span>
            </div>
            <p className="text-xs text-slate-500">{item.reason}</p>
          </div>
        </div>
        <button
          onClick={() => onOpenRun(item.run_id)}
          className="text-xs font-medium text-indigo-600 hover:text-indigo-700"
        >
          查看执行轨迹 →
        </button>
      </div>

      <div className="mt-3 rounded-lg border border-slate-200 bg-slate-50 p-3">
        {parsedArgs && typeof parsedArgs.path === 'string' && typeof parsedArgs.content === 'string' ? (
          <div className="space-y-2">
            <div className="text-xs font-medium text-slate-600">
              目标路径：<span className="font-mono text-slate-900">{parsedArgs.path}</span>
            </div>
            <pre className="max-h-48 overflow-auto whitespace-pre-wrap rounded bg-white p-2.5 font-mono text-xs text-slate-800 border border-slate-200">
              {parsedArgs.content}
            </pre>
          </div>
        ) : parsedArgs && (typeof parsedArgs.code === 'string' || typeof parsedArgs.command === 'string') ? (
          <div className="space-y-1.5">
            <div className="text-xs font-medium text-slate-600">
              {item.tool_name === 'python_exec' ? '即将执行的 Python 代码：' : '即将执行的 Shell 命令：'}
            </div>
            <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-lg bg-slate-900 p-3 font-mono text-xs text-emerald-200">
              {String(parsedArgs.code ?? parsedArgs.command)}
            </pre>
          </div>
        ) : (
          <pre className="max-h-48 overflow-auto whitespace-pre-wrap font-mono text-xs text-slate-800">
            {item.arguments_json}
          </pre>
        )}
      </div>

      <div className="mt-3 flex flex-col gap-2 sm:flex-row sm:items-center">
        <input
          type="text"
          value={feedback}
          onChange={(e) => setFeedback(e.target.value)}
          placeholder="可选：附带修改意见或审批备注..."
          className="flex-1 rounded-lg border border-slate-200 px-3 py-1.5 text-sm outline-none focus:border-indigo-400 focus:ring-2 focus:ring-indigo-100"
        />
        <div className="flex items-center gap-2">
          <button
            disabled={submitting}
            onClick={() => handleAction('rejected')}
            className="inline-flex items-center gap-1 rounded-lg border border-slate-200 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
          >
            <X className="h-4 w-4 text-rose-500" />
            驳回
          </button>
          <button
            disabled={submitting}
            onClick={() => handleAction('approved')}
            className="inline-flex items-center gap-1 rounded-lg bg-emerald-600 px-3.5 py-1.5 text-sm font-medium text-white shadow-xs hover:bg-emerald-700 disabled:opacity-50"
          >
            <Check className="h-4 w-4" />
            批准并继续
          </button>
        </div>
      </div>
    </div>
  );
}
