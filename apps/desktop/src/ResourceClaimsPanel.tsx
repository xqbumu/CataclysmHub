import type { ResourceClaim } from './bindings/ResourceClaim';

const operations: Record<string, string> = {
  'game.launch': '游戏会话',
  'installation.copy': '复制独立安装',
  'world.scan': '存档扫描',
  'backup.create': '创建备份',
  'backup.verify': '校验备份',
  'backup.restore_new': '恢复到新目录',
  'backup.inspect_restored': '检查已恢复目录',
};

export default function ResourceClaimsPanel({
  claims,
  onAcknowledge,
  busy = false,
}: {
  claims: ResourceClaim[];
  onAcknowledge: (id: string) => void;
  busy?: boolean;
}) {
  const byOwner = new Map<string, ResourceClaim>();
  for (const claim of claims) {
    const key = JSON.stringify([claim.owner_kind, claim.owner_id]);
    if (!byOwner.has(key) || claim.scope === 'global') byOwner.set(key, claim);
  }
  const owners = [...byOwner.values()];
  return (
    <section className="panel resource-claims" aria-label="资源占用">
      <h2>资源占用</h2>
      <p className="inline-note">
        普通方案与其他文件任务保持全局互斥；实例备份可检查独立目录，实验构建的固定
        QA
        候选按完整安装与实例范围检查。目录列表仅表示已知保护范围，不能证明游戏没有写入其他位置，正式多版本并发支持尚未验收。
      </p>
      {!owners.length ? (
        <p>当前没有活动资源占用。绑定与存档兼容性仍需单独验证。</p>
      ) : (
        <>
          <p>共有 {owners.length} 个会话或任务正在保护资源。</p>
          {owners.slice(0, 50).map((owner) => (
            <article key={`${owner.owner_kind}:${owner.owner_id}`}>
              <strong>{operations[owner.operation] ?? owner.operation}</strong>
              <p>
                {owner.state === 'interrupted_unknown'
                  ? '状态未知：请检查游戏及其子进程已关闭后，人工确认。'
                  : owner.reason === 'unknown_game_writes'
                    ? '游戏写入范围尚未验证，阻止其他启动和文件任务。'
                    : owner.owner_kind === 'session'
                      ? '会话按声明目录保护资源，重叠范围仍会阻止启动。'
                      : '任务仍在运行或取消中，完成后自动解除占用。'}
              </p>
              {owner.owner_kind === 'session' &&
                owner.state === 'interrupted_unknown' && (
                  <button
                    className="text-button"
                    disabled={busy}
                    onClick={() => onAcknowledge(owner.owner_id)}
                  >
                    检查并确认游戏关闭
                  </button>
                )}
              <code>{owner.owner_id}</code>
              <ul>
                {claims
                  .filter(
                    (claim) =>
                      claim.owner_kind === owner.owner_kind &&
                      claim.owner_id === owner.owner_id &&
                      claim.scope === 'directory',
                  )
                  .map((claim, index) => (
                    <li key={index}>
                      {claim.access === 'write' ? '可能写入' : '读取'}：
                      <code>{claim.path}</code>
                    </li>
                  ))}
              </ul>
            </article>
          ))}
          {owners.length > 50 && (
            <p>仅展示前 50 项，全部占用仍参与冲突判定。</p>
          )}
        </>
      )}
    </section>
  );
}
