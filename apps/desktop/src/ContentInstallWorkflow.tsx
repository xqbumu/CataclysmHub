import { useEffect, useRef, useState } from 'react';
import type { HubApi } from './api';
import { errorMessage } from './api';
import type { ContentInstallPlan } from './bindings/ContentInstallPlan';
import type { ContentInstallTask } from './bindings/ContentInstallTask';
import Modal from './Modal';

export function ContentInstallButton({
  api,
  entryId,
  unavailable,
  onStarted,
}: {
  api: HubApi;
  entryId: string;
  unavailable: boolean;
  onStarted: () => void;
}) {
  const [plan, setPlan] = useState<ContentInstallPlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [ack, setAck] = useState(false);
  const [error, setError] = useState('');
  const requestKey = useRef('');
  async function preview() {
    setBusy(true);
    setError('');
    try {
      const parent = await api.pickPath('directory');
      if (!parent) return;
      setPlan(await api.prepareContentInstall(entryId, parent));
      requestKey.current = crypto.randomUUID();
      setAck(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <button
        disabled={busy || unavailable || !api.desktop}
        onClick={() => void preview()}
      >
        安装到独立资源目录
      </button>
      {error && <p role="alert">{error}</p>}
      {plan && (
        <Modal
          title="确认资源包安装"
          onClose={() => {
            if (!busy) setPlan(null);
          }}
        >
          <p>此操作创建独立资源副本，不写入游戏、不修改世界、不自动启用。</p>
          <p className="file-path">来源：{plan.source_root}</p>
          <p className="file-path">安装容器：{plan.destination}</p>
          <p className="file-path">资源目录：{plan.payload_root}</p>
          <p>
            {plan.files.length} 项 · {plan.total_bytes} 字节
          </p>
          <p className="file-path">完整清单 SHA-256：{plan.manifest_sha256}</p>
          <ul>
            {plan.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
          <details>
            <summary>文件范围（前 200 项）</summary>
            <ul>
              {plan.files.slice(0, 200).map((file) => (
                <li key={file.path}>
                  {file.path} · {file.kind} · {file.size} 字节
                </li>
              ))}
            </ul>
          </details>
          <label>
            <input
              type="checkbox"
              checked={ack}
              onChange={(e) => setAck(e.target.checked)}
            />
            我已核对完整包范围，理解文件一致不等于游戏兼容或已启用。
          </label>
          {error && <p role="alert">{error}</p>}
          <button disabled={busy} onClick={() => setPlan(null)}>
            取消
          </button>
          <button
            disabled={busy || !ack}
            onClick={() => {
              setBusy(true);
              setError('');
              void api
                .startContentInstall(
                  plan.id,
                  plan.plan_digest,
                  requestKey.current,
                  true,
                )
                .then(() => {
                  setPlan(null);
                  onStarted();
                })
                .catch((e: unknown) => setError(errorMessage(e)))
                .finally(() => setBusy(false));
            }}
          >
            确认安装副本
          </button>
        </Modal>
      )}
    </>
  );
}
export function ContentInstallTasks({
  api,
  revision,
}: {
  api: HubApi;
  revision: number;
}) {
  const [tasks, setTasks] = useState<ContentInstallTask[]>([]);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  useEffect(() => {
    if (!api.desktop) return;
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    async function poll() {
      try {
        const next = await api.contentInstallTasks();
        if (!alive) return;
        setTasks(next);
        setError('');
        if (next.some((task) => ['running', 'cancelling'].includes(task.state)))
          timer = setTimeout(() => void poll(), 1000);
      } catch (e) {
        if (alive) setError(errorMessage(e));
      }
    }
    void poll();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [api, revision, busy, refresh]);
  return (
    <section aria-label="资源安装任务">
      <h3>资源安装任务（最近 50 项）</h3>
      <button
        disabled={busy || !api.desktop}
        onClick={() => setRefresh((r) => r + 1)}
      >
        刷新安装任务
      </button>
      {error && <p role="alert">{error}</p>}
      {tasks.map((task) => (
        <article key={task.id} className="panel">
          <p>
            {task.state === 'succeeded'
              ? '完成时副本校验通过 · 游戏兼容性未验证 · 未启用'
              : task.state === 'running'
                ? '复制及校验中'
                : task.state === 'cancelling'
                  ? '正在取消'
                  : task.state === 'cancelled'
                    ? '已取消'
                    : task.state === 'recovery_required'
                      ? '任务中断或结果记录失败，未自动重放'
                      : '安装失败'}
          </p>
          <p className="file-path">来源：{task.source_root}</p>
          <p className="file-path">资源目录：{task.payload_root}</p>
          {task.error_code && (
            <p role="alert">
              {task.error_code}；保留现场，不将未完成目录当作可用资源。
            </p>
          )}
          {task.state === 'succeeded' && (
            <p>
              可以通过“登记本地包目录”选择此资源目录。登记仍不会启用资源；修改世界配置前先建立保护点。
            </p>
          )}
          {['running', 'cancelling'].includes(task.state) && (
            <button
              disabled={busy || task.state === 'cancelling'}
              onClick={() => {
                setBusy(true);
                void api
                  .cancelContentInstall(task.id)
                  .catch((e: unknown) => setError(errorMessage(e)))
                  .finally(() => setBusy(false));
              }}
            >
              取消安装
            </button>
          )}
          {task.error_code === 'TASK_RESULT_PERSIST_FAILED' && (
            <button
              disabled={busy}
              onClick={() => {
                setBusy(true);
                void api
                  .retryTaskRecovery(task.id)
                  .catch((e: unknown) => setError(errorMessage(e)))
                  .finally(() => setBusy(false));
              }}
            >
              重试记录状态
            </button>
          )}
        </article>
      ))}
    </section>
  );
}
