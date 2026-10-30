import type { ContentConfigReport } from './bindings/ContentConfigReport';

export interface ContentConfigDraft {
  defaultMods: string[] | null;
  soundpack: string | null;
  soundEnabled: 'true' | 'false' | null;
}
export interface ConfigDifference {
  path: string;
  field: string;
  before: string[] | string | null;
  after: string[] | string;
  baselineKnown: boolean;
}
export interface ContentConfigPreview {
  differences: ConfigDifference[];
  issues: string[];
}
const validId = (id: string) =>
  id.length > 0 &&
  new TextEncoder().encode(id).length <= 256 &&
  !/[\u0000-\u001f\u007f-\u009f]/u.test(id);

// A local draft against an observed report, never an executable write plan.
export function previewContentConfig(
  report: ContentConfigReport,
  draft: ContentConfigDraft,
): ContentConfigPreview {
  const differences: ConfigDifference[] = [];
  const issues = new Set<string>();
  const addIssue = (message: string) => {
    if (issues.size < 256) issues.add(message);
    else
      issues.add(
        '草稿诊断超过展示上限，仅保留前 256 项；不能据此判断检查完整。',
      );
  };
  const change = (
    path: string,
    field: string,
    before: ConfigDifference['before'],
    after: ConfigDifference['after'],
    known: boolean,
  ) => {
    if (!known || JSON.stringify(before) !== JSON.stringify(after)) {
      differences.push({
        path,
        field,
        before: known ? before : null,
        after,
        baselineKnown: known,
      });
      if (!known)
        addIssue(`${path}：原值未知，不能推断游戏默认值或据此覆盖文件。`);
    }
  };
  const candidates = (kind: string, id: string) =>
    report.discovery.packages
      .filter((pkg) => pkg.kind === kind)
      .flatMap((pkg) => pkg.definitions.filter((def) => def.content_id === id));
  if (draft.defaultMods !== null) {
    const ids = draft.defaultMods;
    if (ids.length > 256 || ids.some((id) => !validId(id))) {
      addIssue('默认模组最多 256 项，每个 ID 须为 1–256 字节且不含控制字符。');
    } else if (new Set(ids).size !== ids.length) {
      addIssue('默认模组存在重复 ID；请明确组合，不自动去重。');
    } else {
      change(
        report.default_mods.path,
        'dependencies（新世界默认组合）',
        report.default_mods.mod_ids,
        ids,
        report.default_mods.status === 'read',
      );
      for (const id of ids) {
        const matches = candidates('mod', id);
        if (matches.length !== 1) {
          addIssue(
            `${id}：${matches.length ? '候选重复，未选择覆盖来源' : '本次候选范围未发现'}。`,
          );
          continue;
        }
        if (matches[0].obsolete)
          addIssue(`${id}：过时模组不能用于新世界默认组合，旧世界引用应保留。`);
        const deps = matches[0].dependencies;
        if (deps === null) addIssue(`${id}：依赖声明未知。`);
        for (const dep of deps ?? []) {
          if (!ids.includes(dep))
            addIssue(`${id}：草稿未列出声明依赖 ${dep}，不自动补齐或排序。`);
        }
      }
      // Existing cycle findings remain observations, not game compatibility rules.
      for (const finding of report.discovery.findings) {
        if (
          finding.code === 'dependency_cycle' &&
          ids.includes(finding.content_id)
        ) {
          addIssue(
            `${finding.content_id} → ${finding.related_id ?? '?'}：候选元数据存在循环依赖。`,
          );
        }
      }
    }
  }
  if (draft.soundpack !== null) {
    if (!validId(draft.soundpack)) {
      addIssue('声音包 ID 须为 1–256 字节且不含控制字符。');
    } else {
      change(
        report.sound.path,
        'SOUNDPACKS',
        report.sound.soundpack,
        draft.soundpack,
        report.sound.status === 'read' && report.sound.soundpack !== null,
      );
      const matches = candidates('soundpack', draft.soundpack);
      if (matches.length !== 1)
        addIssue(
          `声音包 ${draft.soundpack}：${matches.length ? '候选重复，未选择覆盖来源' : '本次候选范围未发现'}。`,
        );
    }
  }
  if (draft.soundEnabled !== null) {
    change(
      report.sound.path,
      'SOUND_ENABLED',
      report.sound.sound_enabled,
      draft.soundEnabled,
      report.sound.status === 'read' && report.sound.sound_enabled !== null,
    );
  }
  if (
    report.discovery.truncated ||
    report.discovery.scopes.some(
      (scope) => !['scanned', 'missing'].includes(scope.status),
    )
  ) {
    addIssue('候选扫描不完整，不能据此确认草稿资源和依赖齐全。');
  }
  return { differences, issues: [...issues] };
}
