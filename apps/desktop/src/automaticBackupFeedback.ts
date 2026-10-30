import type { AutomaticBackupPolicy } from './bindings/AutomaticBackupPolicy';

export function automaticBackupFeedback(policy: AutomaticBackupPolicy) {
  const status = policy.last_status;
  const labels: Record<string, string> = {
    checking: '正在核对',
    started: '已创建备份任务',
    running: '正在备份',
    succeeded: '备份任务已完成',
    failed: '备份失败',
    cancelled: '备份已取消',
    cancelling: '正在取消备份',
    skipped: '本次未生成副本',
    interrupted: '备份已中断',
    recovery_required: '备份中断，需检查',
  };
  const reasons: Record<string, string> = {
    AUTO_BACKUP_MIN_INTERVAL: '尚未到最小间隔，本次未生成新副本。',
    AUTO_BACKUP_EMPTY_SAVE:
      '尚无已保存的游戏进度；请先在游戏内保存并正常退出。',
    AUTO_BACKUP_PROFILE_UNVERIFIED:
      '当前启动方式不支持自动备份，可在关闭游戏后手动备份。',
    AUTO_BACKUP_CONTEXT_CHANGED: '实例来源发生变化，请核对目录后手动备份。',
    AUTO_BACKUP_INTERRUPTED:
      'Hub 曾中断，不会自动补做；请确认游戏关闭后手动备份。',
    GAME_RUNNING_OR_UNKNOWN:
      '游戏仍在运行或状态未知；确认关闭并处理会话状态后再手动备份。',
    TASK_CONFLICT: '另一个文件任务正在占用资源，待其结束后可手动备份。',
    DISK_FULL: '磁盘空间不足，请释放空间或核对备份目录后重试。',
  };
  const code = policy.last_error_code;
  return {
    label: status
      ? (labels[status] ?? `状态待核对（${status}）`)
      : '尚无自动备份记录',
    detail: code
      ? (reasons[code] ?? `未完成，请查看任务记录（${code}）。`)
      : status === 'succeeded'
        ? '任务完成不代表游戏进度已验证，请以副本文件校验记录为准。'
        : '',
    attention:
      !!status &&
      [
        'failed',
        'cancelled',
        'skipped',
        'interrupted',
        'recovery_required',
      ].includes(status),
  };
}
