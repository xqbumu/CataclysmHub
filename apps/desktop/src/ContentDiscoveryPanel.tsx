import type { ContentDiscovery } from './bindings/ContentDiscovery';
const labels: Record<string, string> = {
  scanned: '已完成本范围读取',
  missing: '目录不存在',
  unavailable_base: '基础目录不可用',
  unreadable: '无法读取',
  unsafe_path: '链接或特殊路径，未跟随',
  unsafe_metadata: '元数据不是普通文件',
  invalid_metadata: '元数据损坏或格式不支持',
  unsupported_path: '路径格式不支持',
  partial: '仅部分读取',
  changed: '目录身份变化，已丢弃该范围包记录',
  limit_exceeded: '超过读取限制',
  obsolete_default_mod: '过时模组不能用于新世界默认组合；旧世界引用保留',
  duplicate_id: '重复 ID，未选择覆盖赢家',
  dependencies_unknown: '未声明依赖，依赖情况未知',
  dependency_not_observed: '依赖未在本次候选范围中发现',
  ambiguous_dependency: '依赖有多个候选，无法唯一匹配',
  dependency_cycle: '元数据中存在循环依赖',
  reference_not_observed: '配置引用未在本次候选范围中发现',
  dependency_not_listed: '依赖未在该配置列表中声明',
};
export default function ContentDiscoveryPanel({
  discovery,
}: {
  discovery: ContentDiscovery;
}) {
  return (
    <section>
      <h3>候选资源与依赖诊断</h3>
      <p>
        读取实例 mods、sound 及安装目录默认
        data/mods、data/sound。自定义数据路径和构建前缀未覆盖；不模拟游戏覆盖顺序，不证明实际加载、版本或存档兼容。
      </p>
      {(discovery.truncated ||
        discovery.scopes.some(
          (scope) => !['scanned', 'missing'].includes(scope.status),
        )) && (
        <p role="alert">
          候选扫描不完整，请结合各范围状态判断；未发现不能作为资源不存在的结论。
        </p>
      )}
      <ul>
        {discovery.scopes.map((scope) => (
          <li key={`${scope.kind}:${scope.path}`}>
            <span className="file-path">{scope.path}</span> ·{' '}
            {scope.basis === 'instance_userdir'
              ? '实例用户目录候选'
              : '安装默认目录候选'}{' '}
            · {labels[scope.status] ?? scope.status}
          </li>
        ))}
      </ul>
      <details>
        <summary>读取到的候选包（{discovery.packages.length}）</summary>
        {discovery.packages.map((pkg) => (
          <article key={`${pkg.kind}:${pkg.path}`}>
            <p className="file-path">{pkg.path}</p>
            <p>{pkg.kind === 'mod' ? '模组' : '声音包'}</p>
            <ul>
              {pkg.definitions.map((def) => (
                <li key={def.content_id}>
                  {def.name} · {def.content_id} · 版本声明：
                  {def.version ?? '未知'}
                  {pkg.kind === 'mod' && (
                    <>
                      {' '}
                      · 依赖：
                      {def.dependencies === null
                        ? '未知'
                        : def.dependencies.join(', ') || '显式空列表'}
                    </>
                  )}
                </li>
              ))}
            </ul>
            <p>
              元数据摘要：<code>{pkg.metadata_sha256}</code>
            </p>
          </article>
        ))}
      </details>
      {!discovery.findings.length && (
        <p>本次未产生诊断项；这不代表依赖、游戏加载或存档兼容性验证通过。</p>
      )}
      <ul>
        {discovery.findings.map((finding, index) => (
          <li key={index}>
            {labels[finding.code] ?? finding.code} · {finding.content_id}
            {finding.related_id && <> → {finding.related_id}</>} ·{' '}
            <span className="file-path">{finding.context}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
