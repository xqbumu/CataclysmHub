import { useEffect, useRef, useState } from 'react';
import {
  CircleHelp,
  File,
  Folder,
  FolderOpen,
  Globe2,
  Link,
  LoaderCircle,
  RefreshCw,
  Search,
  Square,
  TriangleAlert,
} from 'lucide-react';
import {
  errorMessage,
  type Dashboard,
  type HubApi,
  type ScanReport,
} from './api';

const issueLabels: Record<string, string> = {
  LINK_NOT_FOLLOWED: '链接未跟随，目标内容未扫描',
  UNSUPPORTED_FILE_TYPE: '不支持的文件类型',
  DIRECTORY_UNREADABLE: '目录不可读或已变化',
  ENTRY_UNREADABLE: '条目不可读或已变化',
  NON_UTF8_NAME: '文件名无法无损显示',
  UNSUPPORTED_NAME: '文件名包含不支持的字符',
  ENTRY_LIMIT: '达到 20,000 个条目的扫描上限',
  DEPTH_LIMIT: '达到 32 层目录深度上限',
};
const taskLabels: Record<string, string> = {
  running: '正在扫描',
  cancelling: '正在安全取消',
  succeeded: '扫描完成',
  cancelled: '已取消',
  failed: '扫描失败',
  recovery_required: '上次扫描中断，请重新扫描',
};
function bytes(value: string) {
  const size = Number(value);
  if (!Number.isFinite(size)) return value + ' B';
  if (size < 1024) return size + ' B';
  if (size < 1024 ** 2) return (size / 1024).toFixed(1) + ' KiB';
  return (size / 1024 ** 2).toFixed(1) + ' MiB';
}
export default function WorldsPage({
  api,
  data,
  busy,
  run,
  onConfigure,
}: {
  api: HubApi;
  data: Dashboard;
  busy: boolean;
  run: (action: () => Promise<void>) => Promise<void>;
  onConfigure: () => void;
}) {
  const [report, setReport] = useState<ScanReport | null>(null);
  const [reportError, setReportError] = useState('');
  const [selectedTask, setSelectedTask] = useState('');
  const [scope, setScope] = useState('all');
  const [search, setSearch] = useState('');
  const [page, setPage] = useState(0);
  const [retry, setRetry] = useState(0);
  const requestKey = useRef<string | null>(null);
  const active = data.scan_tasks.find((task) =>
    ['running', 'cancelling'].includes(task.state),
  );
  const latest = data.scan_tasks[0];
  const completed = data.scan_tasks.filter(
    (task) => task.state === 'succeeded',
  );
  const taskId = selectedTask || completed[0]?.id || '';
  const backupActive = data.backup_tasks.some((task) =>
    ['running', 'cancelling'].includes(task.state),
  );
  const gameActive = data.sessions.some((session) =>
    ['starting', 'running', 'interrupted_unknown'].includes(session.state),
  );
  useEffect(() => {
    let stopped = false;
    setReport(null);
    setReportError('');
    setScope('all');
    setPage(0);
    if (taskId)
      void api
        .scanReport(taskId)
        .then((result) => {
          if (!stopped) setReport(result);
        })
        .catch((error) => {
          if (!stopped) setReportError(errorMessage(error));
        });
    return () => {
      stopped = true;
    };
  }, [api, taskId, retry]);
  const candidates = report?.candidates ?? [];
  const selectedCandidate = candidates.find(
    (candidate) => candidate.id === scope,
  );
  const filtered = (report?.entries ?? []).filter(
    (entry) =>
      (scope === 'all' ||
        (scope === 'shared'
          ? entry.candidate_root === null
          : entry.candidate_root === selectedCandidate?.relative_root)) &&
      entry.relative_path
        .toLocaleLowerCase()
        .includes(search.toLocaleLowerCase()),
  );
  const safePage = Math.min(
    page,
    Math.max(0, Math.ceil(filtered.length / 50) - 1),
  );
  const entries = filtered.slice(safePage * 50, (safePage + 1) * 50);

  if (!data.workspace)
    return (
      <section className="panel empty-library">
        <FolderOpen size={35} />
        <h2>先登记存档目录</h2>
        <p>扫描只读取工作区中明确选择的目录，不搜索设备上的其他位置。</p>
        <button className="primary" onClick={onConfigure}>
          配置工作区
        </button>
      </section>
    );
  return (
    <div className="worlds-page">
      <section className="panel scan-toolbar">
        <div>
          <span className="eyebrow">READ-ONLY DISCOVERY</span>
          <h2>存档目录清单</h2>
          <p className="file-path">{data.workspace.save_dir}</p>
        </div>
        <button
          className="primary"
          disabled={
            busy || !!active || gameActive || backupActive || !api.desktop
          }
          onClick={() =>
            void run(async () => {
              requestKey.current ??= crypto.randomUUID();
              await api.scanWorlds(requestKey.current);
              requestKey.current = null;
              setSelectedTask('');
            })
          }
        >
          <RefreshCw size={16} />
          只读扫描
        </button>
      </section>
      <div className="notice">
        <CircleHelp size={18} />
        <span>
          目录只是世界候选，边界与角色归属尚未验证。扫描不会解析角色内容、改写存档或创建备份；结果也不能证明存档稳定。
        </span>
      </div>
      {backupActive && (
        <div className="notice">
          <CircleHelp size={17} />
          工作区正在备份或校验，请等待完成或取消后再扫描。
        </div>
      )}
      {gameActive && (
        <div className="alert">
          <TriangleAlert size={17} />
          已有游戏运行或状态未知，请关闭并确认后再扫描。
        </div>
      )}
      {latest && (
        <section
          className={`scan-task ${['failed', 'recovery_required'].includes(latest.state) ? 'task-warning' : ''}`}
          aria-live="polite"
        >
          <div>
            {active ? (
              <LoaderCircle size={16} className="spin" />
            ) : (
              <Globe2 size={16} />
            )}
            <strong>{taskLabels[latest.state] ?? latest.state}</strong>
            <span>已枚举 {latest.processed_entries} 个条目</span>
            <span className="mono">{latest.id.slice(0, 8)}</span>
            {latest.error_code && <code>{latest.error_code}</code>}
            {latest.error_code === 'TASK_RESULT_PERSIST_FAILED' && (
              <button
                className="secondary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await api.retryTaskRecovery(latest.id);
                  })
                }
              >
                重试记录任务状态
              </button>
            )}
          </div>
          {active && (
            <button
              className="secondary"
              disabled={busy || active.state === 'cancelling'}
              onClick={() =>
                void run(async () => {
                  await api.cancelScan(active.id);
                })
              }
            >
              <Square size={12} />
              取消扫描
            </button>
          )}
        </section>
      )}
      {completed.length > 0 && (
        <div className="scan-history">
          <label htmlFor="scan-history">查看已完成的扫描</label>
          <select
            id="scan-history"
            value={taskId}
            onChange={(event) => setSelectedTask(event.target.value)}
          >
            {completed.map((task) => (
              <option key={task.id} value={task.id}>
                {new Date(task.created_at).toLocaleString('zh-CN')} ·{' '}
                {task.id.slice(0, 8)}
              </option>
            ))}
          </select>
        </div>
      )}
      {reportError && (
        <div className="alert" role="alert">
          <TriangleAlert size={17} />
          <span>{reportError}</span>
          <button
            className="secondary"
            onClick={() => setRetry((value) => value + 1)}
          >
            重试读取
          </button>
        </div>
      )}
      {taskId && !report && !reportError && (
        <div className="loading" role="status">
          <LoaderCircle size={19} className="spin" />
          读取扫描报告…
        </div>
      )}
      {!taskId && !active && (
        <section className="panel empty-library">
          <Globe2 size={36} />
          <h2>查看世界候选与未知数据</h2>
          <p>
            启动只读扫描后，将展示子目录、文件类型、容量和无法检查的条目。
            <br />
            角色名称和世界身份不会通过文件名自动推断。
          </p>
        </section>
      )}
      {report && (
        <>
          {!report.enumeration_complete && (
            <div className="alert" role="alert">
              <TriangleAlert size={18} />
              <span>
                扫描清单不完整：部分条目不可读、无法表示或达到限制。请查看问题清单；不得将此报告用于完整备份范围。
              </span>
            </div>
          )}
          <div className="scan-summary">
            <span>
              <strong>{candidates.length}</strong> 个目录候选
            </span>
            <span>
              <strong>{report.visited_entries}</strong> 个已枚举条目
            </span>
            <span>
              <strong>{bytes(report.total_bytes)}</strong> 普通文件总大小
            </span>
            <span>
              <strong>{report.issues.length}</strong> 项提示
            </span>
          </div>
          <div className="inventory-layout">
            <aside className="panel candidate-list" aria-label="扫描范围">
              <button
                className={scope === 'all' ? 'candidate-selected' : ''}
                onClick={() => {
                  setScope('all');
                  setPage(0);
                }}
              >
                <Globe2 size={16} />
                全部扫描条目
              </button>
              <button
                className={scope === 'shared' ? 'candidate-selected' : ''}
                onClick={() => {
                  setScope('shared');
                  setPage(0);
                }}
              >
                <File size={16} />
                共享与未归属文件
              </button>
              {candidates.map((candidate) => (
                <button
                  key={candidate.id}
                  className={scope === candidate.id ? 'candidate-selected' : ''}
                  onClick={() => {
                    setScope(candidate.id);
                    setPage(0);
                  }}
                >
                  <Folder size={17} />
                  <div>
                    <strong>{candidate.relative_root}</strong>
                    <small>
                      {candidate.file_count} 个文件 ·{' '}
                      {bytes(candidate.total_bytes)}
                    </small>
                    <small>世界身份未确认 · 角色未知</small>
                  </div>
                </button>
              ))}
            </aside>
            <section className="panel inventory-panel">
              <div className="section-heading">
                <h2>
                  {selectedCandidate
                    ? selectedCandidate.relative_root
                    : scope === 'shared'
                      ? '共享与未归属文件'
                      : '文件清单'}
                </h2>
                <span className="pill neutral">
                  {selectedCandidate ? '边界未验证' : '只读枚举清单'}
                </span>
              </div>
              <label className="inventory-search">
                <Search size={15} />
                <input
                  aria-label="筛选相对路径"
                  placeholder="筛选相对路径…"
                  value={search}
                  onChange={(event) => {
                    setSearch(event.target.value);
                    setPage(0);
                  }}
                />
              </label>
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th>相对路径</th>
                      <th>类型</th>
                      <th>大小</th>
                    </tr>
                  </thead>
                  <tbody>
                    {entries.map((entry) => (
                      <tr key={entry.relative_path}>
                        <td>
                          <span className="inventory-path">
                            {entry.file_type === 'directory' ? (
                              <Folder size={14} />
                            ) : entry.file_type === 'symlink' ? (
                              <Link size={14} />
                            ) : (
                              <File size={14} />
                            )}
                            <span>{entry.relative_path}</span>
                          </span>
                        </td>
                        <td>
                          {{
                            file: '文件',
                            directory: '目录',
                            symlink: '链接',
                            unsupported: '不支持',
                          }[entry.file_type] ?? entry.file_type}
                        </td>
                        <td>
                          {entry.file_type === 'file' ? bytes(entry.size) : '—'}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {entries.length === 0 && (
                <p className="empty-inline">该范围没有匹配条目。</p>
              )}
              <div className="inventory-pagination">
                <span>
                  {filtered.length} 个匹配条目 · 第 {safePage + 1} 页
                </span>
                <div>
                  <button
                    className="secondary"
                    disabled={safePage === 0}
                    onClick={() => setPage(safePage - 1)}
                  >
                    上一页
                  </button>
                  <button
                    className="secondary"
                    disabled={(safePage + 1) * 50 >= filtered.length}
                    onClick={() => setPage(safePage + 1)}
                  >
                    下一页
                  </button>
                </div>
              </div>
            </section>
          </div>
          {report.issues.length > 0 && (
            <section className="panel scan-issues">
              <h2>需要注意的条目</h2>
              <p className="muted">
                链接保留为链接条目且未读取目标；以下提示不会被隐藏为“已验证”。
              </p>
              <ul>
                {report.issues.slice(0, 100).map((issue, index) => (
                  <li key={index}>
                    <TriangleAlert size={14} />
                    <span>{issue.relative_path || '存档根目录'}</span>
                    <strong>{issueLabels[issue.code] ?? issue.code}</strong>
                  </li>
                ))}
              </ul>
              {report.issues.length > 100 && (
                <p className="muted">
                  显示前 100 项，共 {report.issues.length}{' '}
                  项；完整报告保留在本地元数据中。
                </p>
              )}
            </section>
          )}
        </>
      )}
    </div>
  );
}
