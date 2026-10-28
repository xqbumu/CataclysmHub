import Modal from './Modal';
import type { InstanceResourcePreview } from './bindings/InstanceResourcePreview';

const reasons: Record<string, string> = {
  directory_overlap: '物理目录相同或互相嵌套，存在读写冲突',
  unknown_game_writes: '游戏完整写入范围未知，保留全局保护',
  task_serialization: '活动文件任务仍受全局保护',
  scope_unresolved: '无法核实活动操作的资源范围，保守阻止',
};
const issues: Record<string, string> = {
  RESOURCE_SCOPE_UNAVAILABLE: '目录不存在、类型错误或无法访问',
  REQUEST_SCOPE_OVERLAP: '实例目录与安装目录共享或嵌套',
};

export default function InstanceResourcesDialog({
  preview,
  onClose,
}: {
  preview: InstanceResourcePreview;
  onClose: () => void;
}) {
  return (
    <Modal title="实例资源冲突检查" onClose={onClose}>
      <p className="warning">
        此检查只观测已知目录和当前占用，不预留资源。实际绑定未验证，多版本并发尚未开放；启动时会重新检查。
      </p>
      <p>检查时间：{new Date(preview.checked_at).toLocaleString('zh-CN')}</p>
      <h3>本次检查的目录</h3>
      <ul className="instance-resource-scopes">
        {preview.scopes.map((scope) => (
          <li key={scope.role}>
            {scope.role === 'installation'
              ? '安装目录（含共享缓存）'
              : '实例数据目录'}
            {' · 可能写入：'}
            <code>{scope.path}</code>
          </li>
        ))}
      </ul>
      {preview.scope_issues.length > 0 && (
        <div role="alert">
          <h3>目录范围无法确认</h3>
          <ul>
            {preview.scope_issues.map((issue, index) => (
              <li key={index}>
                {issues[issue.code] ?? '目录范围检查失败'}：
                <code>{issue.path}</code>
              </li>
            ))}
          </ul>
        </div>
      )}
      <h3>活动占用冲突 · {preview.conflicts.length} 项</h3>
      {!preview.conflicts.length ? (
        <p>
          {preview.scope_issues.length
            ? '目录存在未解决问题，不能据此判断可启动。'
            : '本次未发现活动占用冲突。这不代表游戏绑定、完整隔离或存档兼容性已验证。'}
        </p>
      ) : (
        <ul className="resource-conflicts">
          {preview.conflicts.slice(0, 50).map((conflict, index) => (
            <li key={index}>
              <strong>
                {reasons[conflict.reason] ?? '存在未识别的资源限制'}
              </strong>
              <p>
                占用者：<code>{conflict.owner_id}</code>
              </p>
              {conflict.requested_path && (
                <p>
                  本实例范围：<code>{conflict.requested_path}</code>
                </p>
              )}
              {conflict.protected_path && (
                <p>
                  受保护范围：<code>{conflict.protected_path}</code>
                </p>
              )}
            </li>
          ))}
          {preview.conflicts.length > 50 && (
            <li>
              仅显示前 50 项，完整冲突数量见标题，未显示的占用同样阻止启动。
            </li>
          )}
        </ul>
      )}
      <div className="modal-actions">
        <button className="secondary" onClick={onClose}>
          关闭检查结果
        </button>
      </div>
    </Modal>
  );
}
