import { useEffect, useState } from 'react';
import { errorMessage, type Dashboard, type HubApi } from './api';
import type { AutomaticBackupPolicy } from './bindings/AutomaticBackupPolicy';
import { automaticBackupFeedback } from './automaticBackupFeedback';

export default function InstanceProtectionSummary({
  api,
  data,
  instanceId,
  onOpenBackups,
  onOpenActivity,
}: {
  api: HubApi;
  data: Dashboard;
  instanceId: string;
  onOpenBackups?: (id: string) => void;
  onOpenActivity?: () => void;
}) {
  const [policy, setPolicy] = useState<AutomaticBackupPolicy | null>(null);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  const sessions = data.sessions.filter(
    (s) => s.play_instance_id === instanceId,
  );
  const sessionIds = new Set(sessions.map((s) => s.id));
  const revision = JSON.stringify([
    sessions.map((s) => [s.id, s.state, s.finished_at]),
    data.backup_tasks.map((t) => [t.id, t.revision]),
    data.operations
      .filter(
        (o) => o.action === 'backup.automatic' && sessionIds.has(o.task_id),
      )
      .map((o) => o.id),
  ]);
  useEffect(() => {
    let current = true;
    setPolicy(null);
    setError('');
    if (api.automaticBackupPolicy)
      void api
        .automaticBackupPolicy(instanceId)
        .then((next) => {
          if (!current) return;
          if (next.instance_id !== instanceId)
            throw new Error('保护状态来源不匹配。');
          setPolicy(next);
        })
        .catch((e) => {
          if (current) setError(errorMessage(e));
        });
    return () => {
      current = false;
    };
  }, [api, instanceId, revision, retry]);
  const backups = data.backups
    .filter((b) => b.play_instance_id === instanceId)
    .sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at));
  const latest = backups[0];
  const verified = backups.find((b) => b.verification === 'verified');
  const feedback = policy ? automaticBackupFeedback(policy) : null;
  const busy =
    data.resource_claims.length > 0 ||
    sessions.some((s) =>
      ['starting', 'running', 'interrupted_unknown'].includes(s.state),
    );
  return (
    <div className="environment-backup" aria-label="实例保护概览">
      <p>
        {verified
          ? `已载入的最近校验通过副本：${new Date(verified.created_at).toLocaleString('zh-CN')}`
          : '当前载入记录中暂无校验通过的副本'}
      </p>
      {latest && latest.verification !== 'verified' && (
        <p role="status">
          最新副本
          {latest.verification === 'unavailable' ? '不可用' : '文件校验未通过'}
          ，请核对后重新备份。
        </p>
      )}
      <p>
        退出后自动备份：
        {policy
          ? policy.settings.enabled
            ? policy.settings.min_interval_minutes === 0
              ? '已开启 · 每次正常退出'
              : `已开启 · 最小间隔 ${policy.settings.min_interval_minutes} 分钟`
            : '未开启'
          : error
            ? '状态未知'
            : api.automaticBackupPolicy
              ? '读取中'
              : '当前接口不可用'}
      </p>
      {feedback && (
        <p role="status">
          最近自动备份：{feedback.label}
          {feedback.detail && ` · ${feedback.detail}`}
        </p>
      )}
      {error && (
        <p role="status">
          保护状态读取失败：{error}
          <button className="secondary" onClick={() => setRetry((n) => n + 1)}>
            重新读取保护状态
          </button>
        </p>
      )}
      {(feedback?.attention ||
        (latest && latest.verification !== 'verified')) && (
        <div className="row-between">
          {onOpenBackups && (
            <button
              className="secondary"
              disabled={busy}
              onClick={() => onOpenBackups(instanceId)}
            >
              前往手动备份
            </button>
          )}
          {onOpenActivity && (
            <button className="secondary" onClick={onOpenActivity}>
              查看备份记录
            </button>
          )}
        </div>
      )}
    </div>
  );
}
