import type { SoundConfiguration } from './bindings/SoundConfiguration';

import { commonGameOptions, languages } from './gameOptions';
export default function GameOptionsFields({
  configuration,
  draft,
  onChange,
}: {
  configuration: SoundConfiguration;
  draft: Record<string, string>;
  onChange: (next: Record<string, string>) => void;
}) {
  const current = configuration.game_options ?? {};
  return (
    <fieldset>
      <legend>常用游戏配置</legend>
      <p>
        仅修改所选实例。语言和自动保存由游戏实现，Hub
        不代替游戏保存；变更后重启游戏核对。可用语言取决于该发行包的翻译资源。
      </p>
      <p className="inline-note">
        自动保存需同时满足回合与现实时间条件（取较大间隔），不是任一计时到达即保存；关闭自动保存时，间隔设置不生效。
      </p>
      {commonGameOptions.map((field) => {
        const available =
          configuration.status === 'read' && Object.hasOwn(current, field.key);
        const editing = Object.hasOwn(draft, field.key);
        const value = draft[field.key] ?? current[field.key] ?? '';
        return (
          <div className="config-section" key={field.key}>
            <label>
              <input
                type="checkbox"
                checked={editing}
                disabled={!available}
                onChange={(e) => {
                  const next = { ...draft };
                  if (e.target.checked) next[field.key] = current[field.key];
                  else delete next[field.key];
                  onChange(next);
                }}
              />
              修改{field.label}
            </label>
            <small>
              当前声明：
              {available
                ? current[field.key] === ''
                  ? '跟随系统（空值）'
                  : current[field.key]
                : '未声明或不可读，请先在游戏中保存设置后重新读取'}
            </small>
            {editing && (
              <label>
                {field.label}
                {field.kind === 'language' ? (
                  <select
                    value={value}
                    onChange={(e) =>
                      onChange({ ...draft, [field.key]: e.target.value })
                    }
                  >
                    {!languages.some(([id]) => id === value) && (
                      <option value={value}>{value}（保留当前语言）</option>
                    )}
                    {languages.map(([id, name]) => (
                      <option value={id} key={id}>
                        {name}
                      </option>
                    ))}
                  </select>
                ) : field.kind === 'boolean' ? (
                  <select
                    value={value}
                    onChange={(e) =>
                      onChange({ ...draft, [field.key]: e.target.value })
                    }
                  >
                    <option value="true">开启</option>
                    <option value="false">关闭</option>
                  </select>
                ) : (
                  <input
                    type="number"
                    min={field.min}
                    max={field.max}
                    step="1"
                    value={value}
                    onChange={(e) =>
                      onChange({ ...draft, [field.key]: e.target.value })
                    }
                  />
                )}
              </label>
            )}
          </div>
        );
      })}
    </fieldset>
  );
}
