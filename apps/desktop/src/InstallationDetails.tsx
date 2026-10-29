import Modal from './Modal';
import type { Dashboard, Installation } from './api';

export default function InstallationDetails({
  installation,
  data,
  onClose,
  onInstances,
}: {
  installation: Installation;
  data: Dashboard;
  onClose: () => void;
  onInstances: () => void;
}) {
  const instances = data.play_instances.filter(
    (i) => i.installation_id === installation.id,
  );
  const ids = new Set(instances.map((i) => i.id));
  const backups = data.backups.filter(
    (b) => b.play_instance_id != null && ids.has(b.play_instance_id),
  );
  const sessions = data.sessions.filter(
    (s) => s.installation_id === installation.id,
  );
  const running = sessions.filter((s) =>
    ['starting', 'running'].includes(s.state),
  );
  const unknown = sessions.filter((s) => s.state === 'interrupted_unknown');
  return (
    <Modal title={installation.name + ' · 版本关联详情'} onClose={onClose} wide>
      <p>
        以下为当前接口返回的安装、实例、备份与近期会话记录，不重新扫描游戏文件，也不保证覆盖全部历史。
      </p>
      <dl className="launch-details">
        <dt>版本／分支</dt>
        <dd>
          {installation.version ?? '未知'} / {installation.branch ?? '未知'}
        </dd>
        <dt>平台</dt>
        <dd>
          {installation.platform} / {installation.arch}
        </dd>
        <dt>程序</dt>
        <dd>{installation.executable}</dd>
        <dt>工作目录</dt>
        <dd>{installation.working_dir}</dd>
      </dl>
      <p>
        {instances.length} 个关联实例 · {backups.length} 个关联备份记录 ·{' '}
        {running.length} 个启动中或运行中会话
      </p>
      {unknown.length > 0 && (
        <p role="alert">
          {unknown.length}{' '}
          个会话状态未知，请在任务与记录中确认；不能据此认定游戏已关闭。
        </p>
      )}
      <p className="inline-note">
        备份按实例 ID
        关联到当前安装登记，不证明备份创建时的游戏版本或跨版本兼容性。工作区备份及没有实例关联的备份不计入。
      </p>
      <h3>关联实例与保护记录</h3>
      {!instances.length && <p>尚未登记关联实例。</p>}
      {instances.map((instance) => {
        const related = backups
          .filter((b) => b.play_instance_id === instance.id)
          .sort((a, b) => b.created_at.localeCompare(a.created_at));
        return (
          <article className="task-center-item" key={instance.id}>
            <h3>{instance.name}</h3>
            <p className="file-path">{instance.data_root}</p>
            <p>目录绑定未验证；保护记录 {related.length} 项。</p>
            {instance.installation_sha256 !== installation.sha256 && (
              <p role="alert">
                实例登记的程序摘要与安装记录不一致。此处未检查磁盘上的当前程序，启动前仍须后端复核。
              </p>
            )}
            {related.length === 0 ? (
              <p>没有关联备份记录；不据此判断磁盘上是否存在其他副本。</p>
            ) : (
              <details>
                <summary>查看 {instance.name} 的备份记录</summary>
                <ul>
                  {related.map((b) => (
                    <li key={b.id}>
                      <strong>{b.display_name || b.id}</strong> · {b.created_at}
                      <p>
                        归档校验记录：{b.verification} · 一致性：{b.consistency}
                      </p>
                      <p className="file-path">备份 ID：{b.id}</p>
                    </li>
                  ))}
                </ul>
                <p>历史校验不保证归档当前仍可用，恢复前须重新验证。</p>
              </details>
            )}
          </article>
        );
      })}
      <details className="disclosure">
        <summary>安装登记标识与摘要</summary>
        <p className="file-path">{installation.id}</p>
        <p className="file-path">{installation.sha256}</p>
        <p>导入时间：{installation.imported_at}</p>
      </details>
      <div className="modal-actions">
        <button className="primary" onClick={onInstances}>
          管理此版本的实例
        </button>
      </div>
    </Modal>
  );
}
