import { useEffect, useState } from 'react';
import {
  AlertCircle,
  Bot,
  CheckCircle2,
  Cpu,
  StopCircle,
  Terminal,
  Wrench,
  X,
} from 'lucide-react';
import { api, withTokenQuery } from '../api';
import type { RunDetail, RunStep } from '../types';
import { StatusBadge } from './StatusBadge';

interface RunTimelineDrawerProps {
  runId: string | null;
  onClose: () => void;
  onUpdated: () => void;
}

export function RunTimelineDrawer({ runId, onClose, onUpdated }: RunTimelineDrawerProps) {
  const [detail, setDetail] = useState<RunDetail | null>(null);
  const [loading, setLoading] = useState(false);

  const fetchDetail = async (id: string) => {
    try {
      const data = await api.getRunDetail(id);
      setDetail(data);
    } catch {
      // ignore transient error
    }
  };

  useEffect(() => {
    if (!runId) {
      setDetail(null);
      return;
    }
    setLoading(true);
    fetchDetail(runId).finally(() => setLoading(false));

    const es = new EventSource(withTokenQuery(`/api/runs/${runId}/events`));
    const refresh = () => {
      fetchDetail(runId);
      onUpdated();
    };
    es.addEventListener('status', refresh);
    es.addEventListener('step', refresh);
    es.addEventListener('approval_needed', refresh);
    es.addEventListener('completed', refresh);
    es.addEventListener('failed', refresh);

    return () => {
      es.close();
    };
  }, [runId]);

  if (!runId) return null;

  const handleCancel = async () => {
    if (!detail) return;
    await api.cancelRun(detail.run.id);
    await fetchDetail(detail.run.id);
    onUpdated();
  };

  const renderStepIcon = (step: RunStep) => {
    switch (step.step_type) {
      case 'thought':
        return <Bot className="h-4 w-4 text-indigo-600" />;
      case 'tool_call':
        return <Wrench className="h-4 w-4 text-amber-600" />;
      case 'tool_result':
        return <Terminal className="h-4 w-4 text-slate-600" />;
      case 'answer':
        return <CheckCircle2 className="h-4 w-4 text-emerald-600" />;
      case 'error':
        return <AlertCircle className="h-4 w-4 text-rose-600" />;
      default:
        return <Cpu className="h-4 w-4 text-slate-500" />;
    }
  };

  const renderStepBody = (step: RunStep) => {
    let payload: Record<string, unknown> = {};
    try {
      payload = JSON.parse(step.payload_json);
    } catch {
      return <pre className="text-xs text-slate-600">{step.payload_json}</pre>;
    }

    if (step.step_type === 'thought' || step.step_type === 'answer') {
      return (
        <div className="whitespace-pre-wrap text-sm leading-relaxed text-slate-800">
          {String(payload.text ?? '')}
        </div>
      );
    }
    if (step.step_type === 'error') {
      return <div className="text-sm text-rose-700">{String(payload.error ?? '')}</div>;
    }
    return (
      <pre className="max-h-56 overflow-auto whitespace-pre-wrap rounded-lg bg-slate-900 p-3 font-mono text-xs text-slate-100">
        {JSON.stringify(payload, null, 2)}
      </pre>
    );
  };

  return (
    <div className="fixed inset-0 z-40 flex justify-end bg-slate-900/30 backdrop-blur-[1px]">
      <div className="flex h-full w-full max-w-2xl flex-col bg-white shadow-2xl">
        <div className="flex items-center justify-between border-b border-slate-200 px-6 py-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-base font-semibold text-slate-900">
                {detail?.task.title ?? '加载执行详情...'}
              </h3>
              {detail && <StatusBadge status={detail.run.status} />}
            </div>
            {detail && (
              <p className="mt-0.5 text-xs text-slate-500">
                步骤 {detail.run.step_count} · Tokens {detail.run.total_tokens} · 触发方式{' '}
                {detail.run.trigger_type}
              </p>
            )}
          </div>
          <div className="flex items-center gap-2">
            {detail &&
              (detail.run.status === 'running' ||
                detail.run.status === 'waiting_approval' ||
                detail.run.status === 'queued') && (
                <button
                  onClick={handleCancel}
                  className="inline-flex items-center gap-1 rounded-lg border border-rose-200 bg-rose-50 px-2.5 py-1.5 text-xs font-medium text-rose-700 hover:bg-rose-100"
                >
                  <StopCircle className="h-3.5 w-3.5" />
                  取消运行
                </button>
              )}
            <button
              onClick={onClose}
              className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-100 hover:text-slate-600"
            >
              <X className="h-5 w-5" />
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-6 py-5">
          {loading && !detail ? (
            <div className="py-12 text-center text-sm text-slate-400">正在加载执行轨迹...</div>
          ) : detail ? (
            <div className="space-y-4">
              <div className="rounded-lg border border-slate-200 bg-slate-50 p-3 text-sm text-slate-700">
                <div className="mb-1 text-xs font-medium uppercase tracking-wider text-slate-400">
                  任务指令
                </div>
                {detail.task.prompt}
              </div>

              <div className="space-y-3">
                {detail.steps.length === 0 ? (
                  <div className="py-6 text-center text-sm text-slate-400">
                    等待 Agent 生成第一步...
                  </div>
                ) : (
                  detail.steps.map((step) => (
                    <div
                      key={step.id}
                      className="rounded-xl border border-slate-200 bg-white p-4 shadow-2xs"
                    >
                      <div className="mb-2 flex items-center justify-between text-xs text-slate-500">
                        <div className="flex items-center gap-2 font-medium text-slate-700">
                          {renderStepIcon(step)}
                          <span>
                            #{step.step_index} · {step.step_type}
                            {step.tool_name ? ` (${step.tool_name})` : ''}
                          </span>
                        </div>
                        <span>{new Date(step.created_at).toLocaleTimeString()}</span>
                      </div>
                      {renderStepBody(step)}
                    </div>
                  ))
                )}
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
