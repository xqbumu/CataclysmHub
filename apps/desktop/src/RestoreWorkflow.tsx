import { useRef, useState } from 'react';
import { FolderOpen, LoaderCircle, Square, TriangleAlert } from 'lucide-react';
import Modal from './Modal';
import InspectionPanel from './InspectionPanel';
import type { HubApi, RestorePlan, RestoreTask, InspectionTask } from './api';
type Runner = (action: () => Promise<void>) => Promise<void>;
export function RestoreButton({
  api,
  backupId,
  busy,
  blocked,
  error,
  run,
}: {
  api: HubApi;
  backupId: string;
  busy: boolean;
  blocked: boolean;
  error: string;
  run: Runner;
}) {
  const [plan, setPlan] = useState<RestorePlan | null>(null);
  const [acknowledged, setAcknowledged] = useState(false);
  const requestKey = useRef<string | null>(null);
  return (
    <>
      <button
        className="secondary"
        disabled={busy || blocked || !api.desktop}
        onClick={() =>
          void run(async () => {
            const parent = await api.pickPath('directory');
            if (!parent) return;
            const preview = await api.restorePlan(backupId, parent);
            setPlan(preview);
            setAcknowledged(false);
            requestKey.current = crypto.randomUUID();
          })
        }
      >
        <FolderOpen size={14} />
        恢复到新目录
      </button>
      {plan && (
        <Modal
          title="确认恢复到新目录"
          onClose={() => {
            if (!busy) setPlan(null);
          }}
        >
          <p className="muted">
            将在所选父目录下创建专用恢复目录。恢复文件位于其中的
            data/，当前存档不会自动切换。
          </p>
          <dl className="launch-details">
            <dt>备份标识</dt>
            <dd>{plan.backup_id}</dd>
            <dt>新建恢复目录</dt>
            <dd className="file-path">{plan.destination}</dd>
            <dt>恢复范围</dt>
            <dd>
              {plan.file_count} 个文件 / {plan.entry_count} 个条目 /{' '}
              {plan.total_bytes} B
            </dd>
            <dt>归档 SHA-256</dt>
            <dd className="hash">{plan.archive_sha256}</dd>
            <dt>备份一致性</dt>
            <dd>{plan.consistency} · 保持原始等级</dd>
            <dt>预览有效期</dt>
            <dd>{new Date(plan.expires_at).toLocaleString('zh-CN')}</dd>
          </dl>
          <div className="plan-warnings">
            <TriangleAlert size={18} />
            <ul>
              {plan.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          </div>
          <label className="backup-ack">
            <input
              type="checkbox"
              checked={acknowledged}
              onChange={(event) => setAcknowledged(event.target.checked)}
            />
            <span>
              我已关闭相关游戏，确认恢复到上述新目录，并理解字节校验不代表游戏兼容性。
            </span>
          </label>
          {error && (
            <p className="modal-error" role="alert">
              {error}
            </p>
          )}
          <div className="modal-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => setPlan(null)}
            >
              取消
            </button>
            <button
              className="primary"
              disabled={busy || blocked || !acknowledged}
              onClick={() =>
                void run(async () => {
                  requestKey.current ??= crypto.randomUUID();
                  await api.startRestore(
                    plan.id,
                    plan.plan_digest,
                    requestKey.current,
                  );
                  setPlan(null);
                })
              }
            >
              {busy && <LoaderCircle size={15} className="spin" />}
              确认恢复到新目录
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
const states: Record<string, string> = {
  running: '执行中',
  cancelling: '正在取消',
  cancelled: '已取消',
  succeeded: '恢复完成 · 当时字节已验证',
  failed: '失败',
  recovery_required: '需要检查目标目录',
};
const phases: Record<string, string> = {
  preflight: '准备恢复',
  verifying_archive: '重新校验归档',
  extracting: '写入新目录',
  checking_restored: '回读验证恢复文件',
  finished: '已结束',
  interrupted: '任务中断',
  recording_failed: '状态记录失败',
};
export function RestoreTasks({
  api,
  tasks,
  inspections = [],
  blocked = false,
  busy,
  run,
}: {
  api: HubApi;
  tasks: RestoreTask[];
  inspections?: InspectionTask[];
  blocked?: boolean;
  busy: boolean;
  run: Runner;
}) {
  if (!tasks.length) return null;
  return (
    <section className="panel" aria-label="新目录恢复任务" aria-live="polite">
      <h3>新目录恢复任务</h3>
      {tasks.map((task) => {
        const active = ['running', 'cancelling'].includes(task.state);
        return (
          <article className="backup-record" key={task.id}>
            <div className="row-between">
              <strong>{states[task.state] ?? task.state}</strong>
              <span>{phases[task.phase] ?? task.phase}</span>
              {active && (
                <button
                  className="secondary"
                  disabled={busy || task.state === 'cancelling'}
                  onClick={() =>
                    void run(async () => {
                      await api.cancelRestore(task.id);
                    })
                  }
                >
                  <Square size={12} />
                  取消恢复
                </button>
              )}
            </div>
            <p className="file-path">{task.destination}</p>
            <small>
              任务 {task.id.slice(0, 8)} · 文件位于 data/ · 不自动切换存档
            </small>
            {task.error_code && (
              <p>
                <code>{task.error_code}</code>
              </p>
            )}
            {['cancelled', 'failed', 'recovery_required'].includes(
              task.state,
            ) && (
              <p className="muted">
                请保留并检查目标目录；其中可能含未完成数据。不会自动清理或重新执行。
              </p>
            )}
            {task.state === 'succeeded' && (
              <p className="muted">
                文件路径、类型、大小和 SHA-256 已回读核对；游戏兼容性仍未验证。
              </p>
            )}
            {!active && (
              <InspectionPanel
                api={api}
                restoreTaskId={task.id}
                inspections={inspections.filter(
                  (inspection) => inspection.restore_task_id === task.id,
                )}
                blocked={blocked}
                busy={busy}
                run={run}
              />
            )}
            {task.error_code === 'TASK_RESULT_PERSIST_FAILED' && (
              <button
                className="secondary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await api.retryTaskRecovery(task.id);
                  })
                }
              >
                重试记录任务状态
              </button>
            )}
          </article>
        );
      })}
    </section>
  );
}
