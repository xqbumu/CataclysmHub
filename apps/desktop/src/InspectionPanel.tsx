import { useRef } from 'react';
import { RefreshCw, Square } from 'lucide-react';
import type { HubApi, InspectionTask } from './api';
const states: Record<string, string> = {
  running: '正在检查目录',
  cancelling: '正在取消检查',
  succeeded: '本次文件检查通过',
  failed: '本次检查未通过',
  cancelled: '检查已取消，未形成新结论',
  recovery_required: '检查中断，需要重新检查',
};
const phases: Record<string, string> = {
  preflight: '准备检查',
  verifying_archive: '重新校验源归档',
  checking_restored: '核对恢复文件',
  recording_failed: '结果记录失败',
};
const failures: Record<string, string> = {
  RESTORED_DATA_MISMATCH:
    '恢复目录的路径、类型或文件内容不一致，或未能完整读取。请保留现有目录并检查。',
  HASH_MISMATCH: '源归档摘要不匹配，请检查备份介质及其他副本。',
  ARCHIVE_INVALID: '源归档格式或内部清单无效，请检查备份副本。',
  BACKUP_IO_FAILED: '无法完整读取相关文件，请检查目录与存储介质。',
  PERMISSION_DENIED: '文件或目录权限不足，请检查所选位置的读取权限。',
  PATH_CHANGED: '已登记路径发生变化，请保留文件并检查位置。',
  PATH_INVALID: '原备份路径不存在或无法访问，请检查介质和目录。',
  RESTORE_MARKER_INVALID: '目录中的历史标记不是普通文件，请保留现场并检查。',
};
export default function InspectionPanel({
  api,
  restoreTaskId,
  inspections,
  busy,
  blocked,
  run,
}: {
  api: HubApi;
  restoreTaskId: string;
  inspections: InspectionTask[];
  busy: boolean;
  blocked: boolean;
  run: (action: () => Promise<void>) => Promise<void>;
}) {
  const requestKey = useRef<string | null>(null);
  const latest = inspections[0];
  const active = latest && ['running', 'cancelling'].includes(latest.state);
  const result = latest?.state === 'succeeded' ? latest.result : null;
  return (
    <section
      className="restore-inspection"
      aria-label="恢复目录检查"
      aria-live="polite"
    >
      <div className="backup-actions">
        <button
          className="secondary"
          disabled={busy || blocked || !!active || !api.desktop}
          onClick={() =>
            void run(async () => {
              requestKey.current ??= crypto.randomUUID();
              await api.startInspection(restoreTaskId, requestKey.current);
              requestKey.current = null;
            })
          }
        >
          <RefreshCw size={14} />
          检查已恢复目录
        </button>
        {active && (
          <button
            className="secondary"
            disabled={busy || latest.state === 'cancelling'}
            onClick={() =>
              void run(async () => {
                await api.cancelInspection(latest.id);
              })
            }
          >
            <Square size={12} />
            取消检查
          </button>
        )}
      </div>
      <p className="muted">
        只读核对归档与恢复文件；检查结果不会改变原恢复任务或自动切换存档。
      </p>
      {latest && (
        <div>
          <strong>{states[latest.state] ?? latest.state}</strong>
          {active && <span> · {phases[latest.phase] ?? latest.phase}</span>}
          <p className="muted">
            本次记录：
            {new Date(latest.finished_at ?? latest.created_at).toLocaleString(
              'zh-CN',
            )}
          </p>
          {latest.error_code && (
            <p>
              <code>{latest.error_code}</code>
            </p>
          )}
          {latest.error_code && failures[latest.error_code] && (
            <p className="muted">{failures[latest.error_code]}</p>
          )}
          {result && (
            <>
              <p>
                {result.verification.restored_files} 个文件 /{' '}
                {result.verification.restored_entries} 个条目 /{' '}
                {result.verification.restored_bytes} B ·{' '}
                {result.verification.backup.consistency}
              </p>
              <p>
                {result.incomplete_marker_present
                  ? '仍有未完成标记；字节匹配不代表原恢复流程已完成。'
                  : '未发现未完成标记；原恢复任务状态保持原记录。'}
              </p>
              <p className="muted">
                历史验证报告：
                {result.historical_report_present
                  ? '存在，仅作历史记录。'
                  : '不存在。'}{' '}
                游戏兼容性仍未验证。
              </p>
              <details>
                <summary>本次检查摘要</summary>
                <dl className="launch-details">
                  <dt>归档 SHA-256</dt>
                  <dd className="hash">
                    {result.verification.backup.archive_sha256}
                  </dd>
                  <dt>清单 SHA-256</dt>
                  <dd className="hash">
                    {result.verification.backup.manifest_sha256}
                  </dd>
                  <dt>检查结束时间</dt>
                  <dd>
                    {new Date(result.inspected_at).toLocaleString('zh-CN')}
                  </dd>
                </dl>
              </details>
            </>
          )}
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
              重试记录检查状态
            </button>
          )}
        </div>
      )}
    </section>
  );
}
