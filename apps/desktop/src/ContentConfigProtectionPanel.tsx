import { useRef, useState } from 'react';
import { errorMessage, type BackupTask, type HubApi } from './api';
import type { ContentConfigInput } from './bindings/ContentConfigInput';
import type { ContentConfigProtectionPlan } from './bindings/ContentConfigProtectionPlan';
export type ProtectionApi = Pick<
  HubApi,
  | 'prepareContentConfigProtection'
  | 'createBackup'
  | 'dashboard'
  | 'cancelBackup'
>;
const states: Record<string, string> = {
  running: '备份执行中',
  cancelling: '正在取消',
  cancelled: '已取消，未建立保护备份',
  failed: '备份失败',
  recovery_required: '任务中断，需要检查',
  succeeded: '备份任务已完成',
};
export default function ContentConfigProtectionPanel({
  api,
  instanceId,
  input,
}: {
  api: ProtectionApi;
  instanceId: string;
  input: ContentConfigInput;
}) {
  const [plan, setPlan] = useState<ContentConfigProtectionPlan | null>(null);
  const [task, setTask] = useState<BackupTask | null>(null);
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const requestKey = useRef('');
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section>
      <h4>为配置变更创建原字节保护备份</h4>
      <p>
        复用完整实例备份，包含配置、存档及其他普通文件；不包含游戏安装。先重新读取草稿并固定原配置摘要，复制时核对；需要明确确认备份范围。不会应用草稿。
      </p>
      <p>
        关闭报告或修改草稿不会取消已提交的备份任务；可在备份页继续查看或取消。
      </p>
      {!task && (
        <button
          disabled={busy}
          onClick={() =>
            void run(async () => {
              setPlan(null);
              setAcknowledged(false);
              const next = await api.prepareContentConfigProtection(
                instanceId,
                input,
              );
              requestKey.current = crypto.randomUUID();
              setPlan(next);
            })
          }
        >
          预览配置保护备份
        </button>
      )}
      {plan && !task && (
        <div aria-label="配置保护备份范围">
          <p>来源：{plan.backup.source_root}</p>
          <p>备份目录：{plan.backup.backup_dir}</p>
          <p>
            整个实例 {plan.backup.file_count} 个文件 · {plan.backup.total_bytes}{' '}
            字节；所需空间 {plan.backup.required_free_bytes} 字节。
          </p>
          <p>
            本次草稿重新读取时间：{plan.configuration.report.checked_at}
            。确认仅创建该来源的备份。
          </p>
          <ul>
            {plan.configuration.files.map((file) => (
              <li key={file.path}>
                {file.path} · 备份复制时必须匹配的原摘要：
                <code>{file.before_sha256}</code>
              </li>
            ))}
          </ul>
          {plan.backup.warnings.map((warning) => (
            <p key={warning}>{warning}</p>
          ))}
          <label>
            <input
              type="checkbox"
              checked={acknowledged}
              disabled={busy}
              onChange={(event) => setAcknowledged(event.target.checked)}
            />
            我确认备份整个实例，并理解尽力快照不保证游戏语义一致
          </label>
          <button
            disabled={busy || !acknowledged}
            onClick={() =>
              void run(async () => {
                setTask(
                  await api.createBackup(
                    plan.backup.id,
                    plan.backup.plan_digest,
                    requestKey.current,
                  ),
                );
              })
            }
          >
            确认创建保护备份
          </button>
        </div>
      )}
      {task && (
        <div aria-live="polite">
          <p>
            {states[task.state] ?? task.state} · {task.phase}
          </p>
          <p>任务：{task.id}</p>
          {task.error_code && <p role="alert">{task.error_code}</p>}
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const next = (await api.dashboard()).backup_tasks.find(
                  (candidate) => candidate.id === task.id,
                );
                if (!next)
                  throw new Error('未找到备份任务，请到备份页检查记录。');
                setTask(next);
              })
            }
          >
            刷新保护备份状态
          </button>
          {task.state === 'running' && (
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  setTask(await api.cancelBackup(task.id));
                })
              }
            >
              取消保护备份
            </button>
          )}
          <p>
            任务记录保留在备份页，可检查、重新校验或恢复到独立目录。未执行配置写入；后续使用前仍须核对备份可用性与当前文件摘要。
          </p>
        </div>
      )}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
