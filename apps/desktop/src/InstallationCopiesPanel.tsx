import { useRef, useState } from 'react';
import Modal from './Modal';
import { type Dashboard, type HubApi, errorMessage } from './api';
import type { ManagedInstallationCopyPlan } from './bindings/ManagedInstallationCopyPlan';
import type { InstallationCopyTask } from './bindings/InstallationCopyTask';
const states: Record<string, string> = {
  running: '执行中',
  cancelling: '取消中',
  succeeded: '已完成',
  failed: '失败',
  cancelled: '已取消',
  recovery_required: '需重新检查',
};
const phases: Record<string, string> = {
  preflight: '检查范围',
  copying: '复制文件',
  checking_source: '复核原安装',
  verifying_copy: '回读副本',
  publishing: '提交副本',
  checking_artifact: '独立核验',
  finished: '已结束',
  interrupted: '重启后保留中断记录',
  recording_failed: '结果记录失败',
};
export default function InstallationCopiesPanel({
  api,
  data,
  onRefresh,
}: {
  api: HubApi;
  data: Dashboard;
  onRefresh: () => Promise<void>;
}) {
  const [sourceId, setSourceId] = useState('');
  const [sourceRoot, setSourceRoot] = useState('');
  const [parent, setParent] = useState('');
  const [plan, setPlan] = useState<ManagedInstallationCopyPlan | null>(null);
  const [ack, setAck] = useState(false);
  const [registering, setRegistering] = useState<InstallationCopyTask | null>(
    null,
  );
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const key = useRef('');
  const blocked = data.resource_claims.length > 0;
  async function perform(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  async function pick(which: 'source' | 'parent') {
    await perform(async () => {
      const path = await api.pickPath('directory');
      if (path) {
        if (which === 'source') setSourceRoot(path);
        else setParent(path);
        setPlan(null);
      }
    });
  }
  return (
    <section className="panel installation-copies">
      <h2>独立安装副本</h2>
      <p>
        将完整安装复制到新目录，保留原件。macOS 可选择 .app
        所在文件夹，预览会明确实际复制根。
      </p>
      <p className="inline-note">
        复制不重写配置中的绝对路径，也不代表签名、绑定或隔离已验证。完成后可独立核验并登记，不自动绑定实例或启动游戏。
      </p>
      <label>
        来源安装
        <select
          value={sourceId}
          disabled={busy}
          onChange={(e) => {
            setSourceId(e.target.value);
            setSourceRoot('');
            setPlan(null);
          }}
        >
          <option value="">选择已登记安装</option>
          {data.installations.map((i) => (
            <option key={i.id} value={i.id}>
              {i.name}
            </option>
          ))}
        </select>
      </label>
      <div className="copy-path">
        <button
          className="secondary"
          disabled={busy || !api.desktop}
          onClick={() => void pick('source')}
        >
          选择完整安装目录
        </button>
        <code>{sourceRoot || '尚未选择来源目录'}</code>
      </div>
      <div className="copy-path">
        <button
          className="secondary"
          disabled={busy || !api.desktop}
          onClick={() => void pick('parent')}
        >
          选择副本存放位置
        </button>
        <code>{parent || '尚未选择目标父目录'}</code>
      </div>
      <button
        className="primary"
        disabled={
          busy || blocked || !api.desktop || !sourceId || !sourceRoot || !parent
        }
        onClick={() =>
          void perform(async () => {
            const next = await api.prepareInstallationCopy(
              sourceId,
              sourceRoot,
              parent,
            );
            setPlan(next);
            setAck(false);
            key.current = crypto.randomUUID();
          })
        }
      >
        {busy ? '正在处理…' : '预览独立安装复制'}
      </button>
      {error && !plan && !registering && <p role="alert">{error}</p>}
      {data.copy_tasks.map((task) => (
        <article className="copy-task" key={task.id}>
          <h3>安装复制 · {states[task.state] ?? task.state}</h3>
          <p>
            {phases[task.phase] ?? task.phase} · 已复制 {task.processed_entries}
            /{task.entry_count} 个条目
          </p>
          <code>{task.destination}</code>
          {task.error_code && <p>{task.error_code}</p>}
          {task.verified_at && (
            <p className="inline-note">
              上次独立核验：{new Date(task.verified_at).toLocaleString('zh-CN')}{' '}
              · 绑定与隔离仍未验证
            </p>
          )}
          {['running', 'cancelling'].includes(task.state) && (
            <button
              className="secondary"
              disabled={
                busy ||
                task.state === 'cancelling' ||
                ['publishing', 'checking_artifact'].includes(task.phase)
              }
              onClick={() =>
                void perform(async () => {
                  await api.cancelInstallationCopy(task.id);
                  await onRefresh();
                })
              }
            >
              取消复制
            </button>
          )}
          {task.error_code === 'TASK_RESULT_PERSIST_FAILED' && (
            <button
              className="secondary"
              disabled={busy}
              onClick={() =>
                void perform(async () => {
                  await api.retryTaskRecovery(task.id);
                  await onRefresh();
                })
              }
            >
              重试记录任务结果
            </button>
          )}
          {task.registered_installation_id ? (
            <p>副本已登记为新安装</p>
          ) : (
            ['succeeded', 'recovery_required'].includes(task.state) && (
              <button
                className="secondary"
                disabled={busy || blocked}
                onClick={() => {
                  setError('');
                  setRegistering(task);
                  const original =
                    data.installations.find(
                      (i) => i.id === task.source_installation_id,
                    )?.name ?? '游戏';
                  setName(
                    Array.from(original).slice(0, 80).join('') + ' · 独立副本',
                  );
                }}
              >
                核验并登记副本
              </button>
            )
          )}
          {task.state === 'recovery_required' && (
            <p className="inline-note">
              不会自动重放复制或清理目录；只有完整副本通过本次独立核验后才能登记，中断记录保留。
            </p>
          )}
        </article>
      ))}
      {plan && (
        <Modal
          title="确认复制独立安装"
          onClose={() => {
            if (!busy) setPlan(null);
          }}
        >
          <dl className="launch-details">
            <dt>实际安装来源</dt>
            <dd>{plan.source_root}</dd>
            <dt>新副本目录</dt>
            <dd>{plan.destination}</dd>
            <dt>完整范围</dt>
            <dd>
              {plan.file_count} 个普通文件、{plan.link_count} 个内部链接，共{' '}
              {plan.total_bytes} 字节
            </dd>
            <dt>程序 SHA-256</dt>
            <dd>{plan.executable_sha256}</dd>
          </dl>
          <ul>
            {plan.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
          <label className="check-label">
            <input
              type="checkbox"
              checked={ack}
              disabled={busy}
              onChange={(e) => setAck(e.target.checked)}
            />
            我已关闭游戏，理解副本仍需独立核验，不代表隔离或并发验证通过。
          </label>
          {error && <p role="alert">{error}</p>}
          <div className="modal-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => setPlan(null)}
            >
              取消预览
            </button>
            <button
              className="primary"
              disabled={busy || !ack}
              onClick={() =>
                void perform(async () => {
                  await api.startInstallationCopy(
                    plan.id,
                    plan.plan_digest,
                    key.current,
                    true,
                  );
                  setPlan(null);
                  await onRefresh();
                })
              }
            >
              确认复制到新目录
            </button>
          </div>
        </Modal>
      )}
      {registering && (
        <Modal
          title="独立核验并登记副本"
          onClose={() => {
            if (!busy) setRegistering(null);
          }}
        >
          <p>
            将重新读取整个副本并核对已确认的清单，成功后才登记新安装。不会绑定实例、启动游戏或开放并发。
          </p>
          <code>{registering.destination}</code>
          <label>
            新安装名称
            <input
              value={name}
              maxLength={100}
              disabled={busy}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          {error && <p role="alert">{error}</p>}
          <div className="modal-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => setRegistering(null)}
            >
              返回
            </button>
            <button
              className="primary"
              disabled={busy || !name.trim()}
              onClick={() =>
                void perform(async () => {
                  await api.registerCopiedInstallation(registering.id, name);
                  setRegistering(null);
                  await onRefresh();
                })
              }
            >
              {busy ? '正在独立核验…' : '确认核验并登记'}
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}
