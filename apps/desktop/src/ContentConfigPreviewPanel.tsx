import { useRef, useState } from 'react';
import { errorMessage, type HubApi } from './api';
import type { ContentConfigPlan } from './bindings/ContentConfigPlan';
import ContentDiscoveryPanel from './ContentDiscoveryPanel';
import type { ContentConfigReport } from './bindings/ContentConfigReport';
import { previewContentConfig } from './contentConfigPreview';

export default function ContentConfigPreviewPanel({
  report,
  api,
}: {
  report: ContentConfigReport;
  api?: Pick<HubApi, 'previewContentConfig'>;
}) {
  const [editMods, setEditMods] = useState(false);
  const [mods, setMods] = useState(
    report.default_mods.status === 'read'
      ? report.default_mods.mod_ids.join('\n')
      : '',
  );
  const [editSound, setEditSound] = useState(false);
  const [sound, setSound] = useState(report.sound.soundpack ?? '');
  const [enabled, setEnabled] = useState<'keep' | 'true' | 'false'>('keep');
  const [preview, setPreview] = useState<ReturnType<
    typeof previewContentConfig
  > | null>(null);
  const revision = useRef(0);
  const [fresh, setFresh] = useState<ContentConfigPlan | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const invalidate = () => {
    revision.current += 1;
    setPreview(null);
    setFresh(null);
    setError('');
  };
  const refreshPreview = async () => {
    if (!api) return;
    const version = ++revision.current;
    setBusy(true);
    setFresh(null);
    setError('');
    try {
      const result = await api.previewContentConfig(report.instance_id, {
        default_mods: editMods
          ? mods
              .split(/\r?\n/u)
              .map((id) => id.trim())
              .filter(Boolean)
          : null,
        soundpack: editSound ? sound.trim() : null,
        sound_enabled: enabled === 'keep' ? null : enabled === 'true',
      });
      if (version === revision.current) setFresh(result);
    } catch (error) {
      if (version === revision.current) setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section>
      <h3>配置草稿与差异预览</h3>
      <p>
        基于本次报告（{report.checked_at}
        ）在内存中比较，不重新读取文件，也不写入。关闭即丢弃；无差异或无提示都不代表兼容验证通过。
      </p>
      <label>
        <input
          type="checkbox"
          checked={editMods}
          onChange={(event) => {
            setEditMods(event.target.checked);
            invalidate();
          }}
        />
        拟调整新世界默认模组
      </label>
      {editMods && (
        <label>
          默认模组 ID（每行一项，空白表示显式空组合）
          <textarea
            value={mods}
            maxLength={66000}
            onChange={(event) => {
              setMods(event.target.value);
              invalidate();
            }}
          />
        </label>
      )}
      <p>
        只比较 config/user-default-mods.json 的默认组合。已有世界
        mods.json、角色、物品和地图不在本草稿范围内；不自动重排依赖。
      </p>
      <label>
        <input
          type="checkbox"
          checked={editSound}
          onChange={(event) => {
            setEditSound(event.target.checked);
            invalidate();
          }}
        />
        拟调整声音包
      </label>
      {editSound && (
        <label>
          目标声音包 ID
          <input
            value={sound}
            maxLength={256}
            onChange={(event) => {
              setSound(event.target.value);
              invalidate();
            }}
          />
        </label>
      )}
      <label>
        声音开关草稿
        <select
          value={enabled}
          onChange={(event) => {
            const value = event.target.value;
            if (value === 'keep' || value === 'true' || value === 'false')
              setEnabled(value);
            invalidate();
          }}
        >
          <option value="keep">保持原声明</option>
          <option value="true">开启（true）</option>
          <option value="false">关闭（false）</option>
        </select>
      </label>
      <button
        onClick={() =>
          setPreview(
            previewContentConfig(report, {
              defaultMods: editMods
                ? mods
                    .split(/\r?\n/u)
                    .map((id) => id.trim())
                    .filter(Boolean)
                : null,
              soundpack: editSound ? sound.trim() : null,
              soundEnabled: enabled === 'keep' ? null : enabled,
            }),
          )
        }
      >
        比较配置草稿
      </button>
      {api && (
        <button disabled={busy} onClick={() => void refreshPreview()}>
          {busy ? '正在重新读取…' : '重新读取并生成后端预览'}
        </button>
      )}
      {error && <p role="alert">{error}</p>}
      {fresh && (
        <div aria-label="后端配置预览">
          <p>
            重新读取时间：{fresh.report.checked_at}
            。下方基于新读取结果；不是一致性快照或执行授权，尚未创建保护副本。
          </p>
          {!fresh.files.length && <p>本次读取未产生配置文件差异。</p>}
          {fresh.files.map((file) => (
            <article key={file.path}>
              <p>{file.path}</p>
              <p>
                原文件 SHA-256：<code>{file.before_sha256}</code>
              </p>
              <p>
                拟生成文件 SHA-256：<code>{file.proposed_sha256}</code>
              </p>
              <details>
                <summary>拟生成 JSON（仅展示）</summary>
                <pre>{file.proposed_json}</pre>
              </details>
            </article>
          ))}
          <ContentDiscoveryPanel discovery={fresh.proposed_discovery} />
          <p>只生成预览，不提供确认写入；已有世界配置保持在预览范围之外。</p>
        </div>
      )}
      {preview && (
        <div aria-label="配置草稿结果">
          {preview.issues.map((issue) => (
            <p role="alert" key={issue}>
              {issue}
            </p>
          ))}
          {!preview.differences.length && (
            <p>没有可展示的字段差异；如有输入问题，请先修正。</p>
          )}
          {preview.differences.map((diff) => (
            <article key={`${diff.path}:${diff.field}`}>
              <p>
                {diff.path} · {diff.field}
              </p>
              <p>
                原声明：
                {diff.baselineKnown
                  ? JSON.stringify(diff.before)
                  : '未知（未读取或未声明），不推断默认值'}
              </p>
              <p>草稿：{JSON.stringify(diff.after)}</p>
            </article>
          ))}
          <p>
            后续执行前须退出目标游戏、重新读取并核对配置与资源、为目标配置建立可验证的原文件保护副本。涉及已有世界或版本迁移时，还须备份存档并验证恢复；本预览未创建或检查保护点，不能作为写入授权。
          </p>
          <p>
            请先在独立实验实例验证新世界创建与声音播放；本轮不提供应用草稿按钮。
          </p>
        </div>
      )}
    </section>
  );
}
