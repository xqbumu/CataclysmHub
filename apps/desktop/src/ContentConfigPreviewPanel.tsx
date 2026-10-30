import { contentChoices } from './contentChoices';
import ConfigChangePanel, { type ConfigChangeApi } from './ConfigChangePanel';
import ContentConfigProtectionPanel, {
  type ProtectionApi,
} from './ContentConfigProtectionPanel';
import type { ContentConfigInput } from './bindings/ContentConfigInput';
import { useRef, useState } from 'react';
import { errorMessage, type HubApi } from './api';
import type { ContentConfigPlan } from './bindings/ContentConfigPlan';
import ContentDiscoveryPanel from './ContentDiscoveryPanel';
import type { ContentConfigReport } from './bindings/ContentConfigReport';
import { previewContentConfig } from './contentConfigPreview';

export default function ContentConfigPreviewPanel({
  report,
  api,
  protectionApi,
  changeApi,
}: {
  report: ContentConfigReport;
  api?: Pick<HubApi, 'previewContentConfig'>;
  protectionApi?: ProtectionApi;
  changeApi?: ConfigChangeApi;
}) {
  const modChoices = contentChoices(report, 'mod');
  const soundChoices = contentChoices(report, 'soundpack');
  const [addMod, setAddMod] = useState('');
  const selectedMods = (text: string) =>
    text
      .split(/\r?\n/u)
      .map((id) => id.trim())
      .filter(Boolean);
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
  const [freshInput, setFreshInput] = useState<ContentConfigInput | null>(null);
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
      const input: ContentConfigInput = {
        default_mods: editMods
          ? mods
              .split(/\r?\n/u)
              .map((id) => id.trim())
              .filter(Boolean)
          : null,
        soundpack: editSound ? sound.trim() : null,
        sound_enabled: enabled === 'keep' ? null : enabled === 'true',
      };
      const result = await api.previewContentConfig(report.instance_id, input);
      if (version === revision.current) {
        setFresh(result);
        setFreshInput(input);
      }
    } catch (error) {
      if (version === revision.current) setError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section>
      <h3>1. 选择要修改的配置</h3>
      <p>
        基于本次报告（{report.checked_at}
        ）在内存中比较，不重新读取文件，也不写入。关闭即丢弃；无差异或无提示都不代表兼容验证通过。
      </p>
      <label>
        <input
          type="checkbox"
          checked={editMods}
          disabled={report.default_mods.status !== 'read'}
          onChange={(event) => {
            setEditMods(event.target.checked);
            invalidate();
          }}
        />
        拟调整新世界默认模组
      </label>
      {report.default_mods.status !== 'read' && (
        <p role="status">
          默认模组配置尚不可编辑：
          {report.default_mods.status === 'missing'
            ? '请先启动游戏，在游戏中保存默认模组设置，再重新读取。'
            : '请检查文件读取或格式问题，再重新读取；不会覆盖异常文件。'}
        </p>
      )}
      {editMods && (
        <div>
          <p className="inline-note">
            候选来自此实例与关联安装的扫描范围，资源库登记不代表游戏可发现。重复
            ID 不自动选择；现有顺序保留，不自动补依赖。
          </p>
          <label>
            添加可用模组
            <select value={addMod} onChange={(e) => setAddMod(e.target.value)}>
              <option value="">选择模组</option>
              {modChoices.map((choice) => (
                <option
                  key={choice.id}
                  value={choice.id}
                  disabled={
                    choice.obsolete ||
                    choice.paths.length > 1 ||
                    selectedMods(mods).includes(choice.id)
                  }
                >
                  {choice.name} · {choice.id}
                  {choice.obsolete
                    ? '（过时，仅供旧存档）'
                    : choice.paths.length > 1
                      ? '（重复 ID）'
                      : ''}
                </option>
              ))}
            </select>
          </label>
          <button
            disabled={!addMod || selectedMods(mods).includes(addMod)}
            onClick={() => {
              setMods([...selectedMods(mods), addMod].join('\n'));
              setAddMod('');
              invalidate();
            }}
          >
            添加模组
          </button>
          <ol aria-label="新世界模组顺序">
            {selectedMods(mods).map((id, index, ids) => {
              const choice = modChoices.find((c) => c.id === id);
              const move = (offset: number) => {
                const next = [...ids];
                [next[index], next[index + offset]] = [
                  next[index + offset],
                  next[index],
                ];
                setMods(next.join('\n'));
                invalidate();
              };
              return (
                <li key={`${id}:${index}`}>
                  <strong>{choice?.name ?? id}</strong> · {id}
                  {choice ? (
                    <small>
                      {' '}
                      版本声明：{choice.version ?? '未声明'} · 依赖：
                      {choice.dependencies?.join(', ') || '未声明'} ·{' '}
                      {choice.paths.length > 1
                        ? '重复 ID，需排查'
                        : choice.paths[0]}
                    </small>
                  ) : (
                    <small> 本次扫描未发现，请先检查资源目录</small>
                  )}
                  <button
                    aria-label={`上移 ${id}`}
                    disabled={index === 0}
                    onClick={() => move(-1)}
                  >
                    上移
                  </button>
                  <button
                    aria-label={`下移 ${id}`}
                    disabled={index === ids.length - 1}
                    onClick={() => move(1)}
                  >
                    下移
                  </button>
                  <button
                    aria-label={`移除 ${id}`}
                    onClick={() => {
                      setMods(ids.filter((_, i) => i !== index).join('\n'));
                      invalidate();
                    }}
                  >
                    移除
                  </button>
                </li>
              );
            })}
          </ol>
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
        </div>
      )}
      <p>
        只比较 config/user-default-mods.json 的默认组合。已有世界
        mods.json、角色、物品和地图不在本草稿范围内；不自动重排依赖。
      </p>
      <label>
        <input
          type="checkbox"
          checked={editSound}
          disabled={report.sound.status !== 'read'}
          onChange={(event) => {
            setEditSound(event.target.checked);
            invalidate();
          }}
        />
        拟调整声音包
      </label>
      {report.sound.status !== 'read' && (
        <p role="status">
          声音配置尚不可编辑：
          {report.sound.status === 'missing'
            ? '请先启动游戏并保存设置，再重新读取。'
            : '请检查文件读取或格式问题，再重新读取；不会覆盖异常文件。'}
        </p>
      )}
      {editSound && (
        <div>
          <label>
            选择已发现声音包
            <select
              value={soundChoices.some((c) => c.id === sound) ? sound : ''}
              onChange={(e) => {
                if (e.target.value) {
                  setSound(e.target.value);
                  invalidate();
                }
              }}
            >
              <option value="">选择声音包（保留当前草稿）</option>
              {soundChoices.map((choice) => (
                <option
                  key={choice.id}
                  value={choice.id}
                  disabled={choice.paths.length > 1}
                >
                  {choice.name} · {choice.id}
                  {choice.paths.length > 1 ? '（重复 ID）' : ''}
                </option>
              ))}
            </select>
          </label>
          <p className="inline-note">
            声音包须位于目标实例或安装可发现目录；这里只修改该实例的配置，不写入其他实例。切换声音包后需重启游戏，实际播放仍需在游戏中验证。
          </p>
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
        </div>
      )}
      <label>
        声音开关草稿
        <select
          value={enabled}
          disabled={report.sound.status !== 'read'}
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
        <div aria-label="后端配置预览" className="config-section">
          <h3>2. 核对差异与保护副本</h3>
          <p>
            重新读取时间：{fresh.report.checked_at}
            。下方基于新读取结果；不是一致性快照或执行授权。预览本身不创建保护副本，可在下方单独创建备份。
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
          <details
            className="disclosure"
            open={
              fresh.proposed_discovery.findings.length > 0 ||
              fresh.proposed_discovery.truncated
            }
          >
            <summary>
              本次变更诊断 · {fresh.proposed_discovery.findings.length} 项
            </summary>
            <ContentDiscoveryPanel discovery={fresh.proposed_discovery} />
          </details>
          {protectionApi && freshInput && fresh.files.length > 0 && (
            <ContentConfigProtectionPanel
              api={protectionApi}
              instanceId={report.instance_id}
              input={freshInput}
            />
          )}
          {changeApi && freshInput && fresh.files.length === 1 && (
            <ConfigChangePanel
              api={changeApi}
              instanceId={report.instance_id}
              input={freshInput}
            />
          )}
          <p>
            预览本身不执行写入；单文件应用须在独立入口重新校验并确认，已有世界配置不在范围内。
          </p>
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
            请先在独立实验实例验证新世界创建与声音播放；应用前须使用后端单文件准备与确认入口。
          </p>
        </div>
      )}
    </section>
  );
}
