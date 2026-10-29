import { useEffect, useState } from 'react';
import { errorMessage, type Dashboard, type HubApi } from './api';
import type { ContentInstallTask } from './bindings/ContentInstallTask';

export type TaskDestination = 'archives' | 'worlds' | 'copies' | 'content';
type Task = {
  id: string;
  state: string;
  phase: string;
  error_code: string | null;
  created_at: string;
  finished_at: string | null;
};
type Row = Task & {
  kind: string;
  target: string;
  destination: TaskDestination;
};
const states: Record<string, string> = {
  running: '执行中',
  cancelling: '正在取消',
  cancelled: '已取消',
  succeeded: '任务完成',
  failed: '失败',
  recovery_required: '中断，需要检查',
};
function group(state: string) {
  if (['running', 'cancelling'].includes(state)) return 'active';
  if (['succeeded', 'cancelled'].includes(state)) return 'finished';
  return 'attention';
}
const rank = { attention: 0, active: 1, finished: 2 };
export default function TaskCenter({
  data,
  api,
  onOpen,
}: {
  data: Dashboard;
  api: HubApi;
  onOpen: (destination: TaskDestination) => void;
}) {
  const [content, setContent] = useState<ContentInstallTask[]>([]);
  const [loaded, setLoaded] = useState(!api.desktop);
  const [error, setError] = useState('');
  const [revision, setRevision] = useState(0);
  const [filter, setFilter] = useState('pending');
  const [query, setQuery] = useState('');
  useEffect(() => {
    if (!api.desktop) return;
    let alive = true;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const result = await api.contentInstallTasks();
        if (alive) {
          setContent(result);
          setLoaded(true);
          setError('');
        }
      } catch (e) {
        if (alive) setError(errorMessage(e));
      } finally {
        if (alive) timer = setTimeout(() => void poll(), 2000);
      }
    }
    void poll();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [api, revision]);
  const rows: Row[] = [
    ...data.scan_tasks.map((t) => ({
      ...t,
      kind: '世界扫描',
      target: t.workspace_id,
      destination: 'worlds' as const,
    })),
    ...data.backup_tasks.map((t) => ({
      ...t,
      kind: t.operation === 'backup.verify' ? '归档校验' : '备份',
      target: t.source_root ?? t.backup_id,
      destination: 'archives' as const,
    })),
    ...data.restore_tasks.map((t) => ({
      ...t,
      kind: '恢复',
      target: t.destination,
      destination: 'archives' as const,
    })),
    ...data.inspection_tasks.map((t) => ({
      ...t,
      kind: '恢复目录核验',
      target: t.restore_task_id,
      destination: 'archives' as const,
    })),
    ...data.copy_tasks.map((t) => ({
      ...t,
      kind: '安装复制',
      target: t.destination,
      destination: 'copies' as const,
    })),
    ...content.map((t) => ({
      ...t,
      kind: '资源安装',
      target: t.destination,
      destination: 'content' as const,
    })),
  ].sort(
    (a, b) =>
      rank[group(a.state)] - rank[group(b.state)] ||
      b.created_at.localeCompare(a.created_at),
  );
  const visible = rows.filter(
    (t) =>
      (filter === 'all' ||
        (filter === 'pending' && group(t.state) !== 'finished') ||
        group(t.state) === filter) &&
      [t.id, t.kind, t.target, t.error_code ?? '']
        .join(' ')
        .toLowerCase()
        .includes(query.trim().toLowerCase()),
  );
  return (
    <section className="panel" aria-label="任务中心">
      <h2>任务中心</h2>
      <p className="muted">
        汇总各接口返回的近期文件任务，每两秒刷新。任务完成不代表游戏兼容；恢复目录核验不改变原恢复结果。
      </p>
      <p>
        需要处理 {rows.filter((t) => group(t.state) === 'attention').length} ·
        执行中 {rows.filter((t) => group(t.state) === 'active').length} · 已结束{' '}
        {rows.filter((t) => group(t.state) === 'finished').length}
      </p>
      <div className="task-center-filters">
        <label>
          任务状态
          <select value={filter} onChange={(e) => setFilter(e.target.value)}>
            <option value="pending">需要关注（含执行中）</option>
            <option value="all">全部近期任务</option>
            <option value="attention">需要处理</option>
            <option value="active">执行中</option>
            <option value="finished">已结束</option>
          </select>
        </label>
        <label>
          搜索任务
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="任务 ID、类型、路径或错误码"
          />
        </label>
      </div>
      {!loaded && !error && <p role="status">正在读取资源安装任务…</p>}
      {error && (
        <div role="alert">
          <p>
            资源安装任务读取失败，列表可能不完整；已有记录为上次读取结果。
            {error}
          </p>
          <button onClick={() => setRevision((v) => v + 1)}>
            重试资源任务读取
          </button>
        </div>
      )}
      {!visible.length && (
        <p>
          {rows.length
            ? filter === 'pending' && !query.trim()
              ? '当前没有需要关注的任务，可切换到已结束查看记录。'
              : '没有匹配的任务。'
            : loaded && !error
              ? '近期没有文件任务。'
              : '当前已读取的范围内没有文件任务。'}
        </p>
      )}
      <div className="task-center-list">
        {visible.map((t) => (
          <article className="task-center-item" key={t.kind + ':' + t.id}>
            <div className="section-heading">
              <h3>
                {t.kind} · {states[t.state] ?? '未知状态：' + t.state}
              </h3>
              <button onClick={() => onOpen(t.destination)}>
                打开{t.kind}管理
              </button>
            </div>
            <p className="file-path">{t.target}</p>
            <p>
              阶段：{t.phase} · 创建：{t.created_at}
            </p>
            {t.error_code && <p role="status">错误码：{t.error_code}</p>}
            {group(t.state) === 'attention' && (
              <p>请在对应管理页检查记录；此处不会自动重试或解除保护。</p>
            )}
            <details>
              <summary>任务标识与结束时间</summary>
              <code>{t.id}</code>
              <p>{t.finished_at ?? '尚无结束时间'}</p>
            </details>
          </article>
        ))}
      </div>
    </section>
  );
}
