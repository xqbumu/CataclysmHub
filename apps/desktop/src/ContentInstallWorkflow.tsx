import type { PlayInstance } from './bindings/PlayInstance';
import type { Installation } from './bindings/Installation';
import type { InstanceContentInstallPlan } from './bindings/InstanceContentInstallPlan';
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
  instances = [],
  installations = [],
}: {
  api: HubApi;
  entryId: string;
  unavailable: boolean;
  onStarted: () => void;
  instances?: PlayInstance[];
  installations?: Installation[];
}) {
  const [plan, setPlan] = useState<ContentInstallPlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [ack, setAck] = useState(false);
  const [error, setError] = useState('');
  const [choosingInstance, setChoosingInstance] = useState(false);
  const [instanceId, setInstanceId] = useState('');
  const [deployment, setDeployment] =
    useState<InstanceContentInstallPlan | null>(null);
  const requestKey = useRef('');
  async function preview() {
    setBusy(true);
    setError('');
    try {
      const parent = await api.pickPath('directory');
      if (!parent) return;
      setDeployment(null);
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
      <button
        disabled={busy || unavailable || !api.desktop || !instances.length}
        onClick={() => {
          setChoosingInstance(true);
          setError('');
        }}
      >
        部署到游玩实例
      </button>
      {error && !plan && !choosingInstance && <p role="alert">{error}</p>}
      {choosingInstance && (
        <Modal
          title="选择资源部署目标"
          onClose={() => {
            if (!busy) setChoosingInstance(false);
          }}
        >
          <label>
            目标游玩实例
            <select
              disabled={busy}
              value={instanceId}
              onChange={(e) => setInstanceId(e.target.value)}
            >
              <option value="">选择实例及关联游戏</option>
              {instances.map((instance) => {
                const game = installations.find(
                  (i) => i.id === instance.installation_id,
                );
                return (
                  <option key={instance.id} value={instance.id}>
                    {instance.name} · {game?.name ?? instance.installation_id} ·{' '}
                    {game?.version ?? '版本未知'}
                  </option>
                );
              })}
            </select>
          </label>
          <p>
            复制到所选实例的 mods 或 sound
            新目录；不修改游戏安装、已有资源或世界组合。部署后重新读取该实例配置，再选择模组或声音。
          </p>
          <p className="inline-note">
            当前发布支持 macOS/Linux；其他系统由后端拒绝，尚未完成运行验收。
          </p>
          {error && <p role="alert">{error}</p>}
          <button disabled={busy} onClick={() => setChoosingInstance(false)}>
            取消
          </button>
          <button
            disabled={busy || !instances.some((i) => i.id === instanceId)}
            onClick={() => {
              setBusy(true);
              setError('');
              void api
                .prepareInstanceContentInstall(entryId, instanceId)
                .then((result) => {
                  if (
                    result.instance_id !== instanceId ||
                    result.plan.entry_id !== entryId
                  )
                    throw new Error('部署计划与目标不一致，请重新预览。');
                  setDeployment(result);
                  setPlan(result.plan);
                  setChoosingInstance(false);
                  setAck(false);
                  requestKey.current = crypto.randomUUID();
                })
                .catch((e) => setError(errorMessage(e)))
                .finally(() => setBusy(false));
            }}
          >
            {busy ? '正在检查资源范围…' : '预览实例部署'}
          </button>
        </Modal>
      )}
      {plan && (
        <Modal
          title={deployment ? '确认实例资源部署' : '确认资源包安装'}
          onClose={() => {
            if (!busy) setPlan(null);
          }}
        >
          {deployment ? (
            <p>
              目标实例：{deployment.instance_name} · {deployment.instance_id}
              。只部署资源文件，不修改游戏安装或世界组合，不自动启用。
            </p>
          ) : (
            <p>此操作创建独立资源副本，不写入游戏、不修改世界、不自动启用。</p>
          )}
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
            {deployment ? '确认部署到此实例' : '确认安装副本'}
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
              ? '完成时副本校验通过 · 游戏兼容性未验证 · 配置未修改'
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
              独立资源库副本可通过“登记本地包目录”登记；若部署到实例，请重新读取该实例配置并选择资源。此任务不自动启用，修改配置前仍需保护副本。
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
