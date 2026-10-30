export const commonGameOptions = [
  { key: 'USE_LANG', label: '游戏语言', kind: 'language' },
  { key: 'AUTOSAVE', label: '游戏自动保存', kind: 'boolean' },
  {
    key: 'AUTOSAVE_TURNS',
    label: '自动保存间隔（游戏回合）',
    kind: 'number',
    min: 10,
    max: 1000,
  },
  {
    key: 'AUTOSAVE_MINUTES',
    label: '自动保存间隔（现实分钟）',
    kind: 'number',
    min: 0,
    max: 127,
  },
] as const;
export const languages = [
  ['', '跟随系统'],
  ['en', 'English'],
  ['zh_CN', '简体中文'],
  ['zh_TW', '繁體中文'],
  ['ja', '日本語'],
  ['ko', '한국어'],
  ['de', 'Deutsch'],
  ['fr', 'Français'],
  ['es', 'Español'],
  ['ru', 'Русский'],
];
