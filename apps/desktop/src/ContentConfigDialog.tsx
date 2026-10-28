import type { ContentConfigReport } from './bindings/ContentConfigReport';
import type { ModReferenceFile } from './bindings/ModReferenceFile';
import Modal from './Modal';
import ContentDiscoveryPanel from './ContentDiscoveryPanel';
const statuses: Record<string, string> = {
  read: '已读取声明',
  missing: '文件或目录缺失，不能推断默认值',
  unreadable: '无法读取',
  unsafe_path: '链接或非普通路径，未跟随',
  invalid_json: 'JSON 损坏或正在变化',
  unsupported_format: '格式不支持或字段重复',
  limit_exceeded: '超过读取限制',
};
function Mods({ file }: { file: ModReferenceFile }) {
  return (
    <article>
      <p className="file-path">{file.path}</p>
      <p>{statuses[file.status] ?? file.status}</p>
      {file.status === 'read' && (
        <p>
          声明顺序：
          {file.mod_ids.length ? file.mod_ids.join(' → ') : '显式空列表'}
        </p>
      )}
      {file.sha256 && (
        <details>
          <summary>读取摘要</summary>
          <code>{file.sha256}</code>
        </details>
      )}
    </article>
  );
}
export default function ContentConfigDialog({
  report,
  onClose,
}: {
  report: ContentConfigReport;
  onClose: () => void;
}) {
  return (
    <Modal title="模组与声音配置引用" onClose={onClose}>
      <p>
        只读观察 Hub 实例目录中的 CDDA 配置格式。安装版本记录：
        {report.installation_version ?? '未知'}
        ；绑定和兼容性未验证，不证明游戏实际发现或加载了资源。
      </p>
      <p>
        检查时间：{report.checked_at}
        。游戏或外部工具可能同时改写文件，本报告不是一致性快照，也不授权修改或卸载。
      </p>
      <p className="file-path">{report.data_root}</p>
      {report.issues.map((issue) => (
        <p role="alert" key={issue}>
          {issue}；扫描范围不完整。
        </p>
      ))}
      <h3>声音设置声明</h3>
      <p>
        {report.sound.path} ·{' '}
        {statuses[report.sound.status] ?? report.sound.status}
      </p>
      {report.sound.status === 'read' && (
        <>
          <p>声音包：{report.sound.soundpack ?? '未声明，不推断默认包'}</p>
          <p>SOUND_ENABLED 原值：{report.sound.sound_enabled ?? '未声明'}</p>
        </>
      )}
      {report.sound.sha256 && (
        <details>
          <summary>声音配置读取摘要</summary>
          <code>{report.sound.sha256}</code>
        </details>
      )}
      <h3>新世界默认模组声明</h3>
      <p>此列表不代表已有世界的模组组合。</p>
      <Mods file={report.default_mods} />
      <h3>世界目录候选中的模组声明</h3>
      {!report.worlds.length && (
        <p>本次没有可展示的目录候选；请结合扫描范围提示判断。</p>
      )}
      {report.worlds.map((file) => (
        <Mods key={file.path} file={file} />
      ))}
      <h3>资源引用与潜在影响范围</h3>
      <p>
        升级或移除某 ID
        前，先核对下面引用它的配置并建立保护点。资源库匹配来自登记快照，不是目标游戏的资源发现结果；候选目录的本次读取结果见下方。
      </p>
      {!report.references.length && (
        <p>本次成功读取的文件中未提取到引用；缺失或失败文件可能仍包含引用。</p>
      )}
      {report.references.map((ref) => (
        <article key={`${ref.kind}:${ref.content_id}`}>
          <strong>
            {ref.kind === 'mod' ? '模组' : '声音包'} · {ref.content_id}
          </strong>
          <ul>
            {ref.referenced_by.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
          {ref.registered_roots.length ? (
            <>
              <p>
                资源库匹配 {ref.registered_roots.length} 处
                {ref.registered_roots.length > 1
                  ? '，存在歧义，不能自动选择'
                  : ''}
                ：
              </p>
              <ul>
                {ref.registered_roots.map((path) => (
                  <li key={path} className="file-path">
                    {path}
                  </li>
                ))}
              </ul>
            </>
          ) : (
            <p>资源库未登记匹配项，不代表目标游戏缺少该资源。</p>
          )}
        </article>
      ))}
      <ContentDiscoveryPanel discovery={report.discovery} />
      <button onClick={onClose}>关闭</button>
    </Modal>
  );
}
