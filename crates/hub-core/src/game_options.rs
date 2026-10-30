//! Small allowlist for common options observed in CDDA H/I; never synthesize missing fields.
pub(crate) const KEYS: &[&str] = &["USE_LANG", "AUTOSAVE", "AUTOSAVE_TURNS", "AUTOSAVE_MINUTES"];
pub(crate) fn valid(key: &str, value: &str) -> bool {
    match key {
        "USE_LANG" => matches!(
            value,
            "" | "en" | "zh_CN" | "zh_TW" | "ja" | "ko" | "de" | "fr" | "es" | "ru"
        ),
        "AUTOSAVE" => matches!(value, "true" | "false"),
        "AUTOSAVE_TURNS" | "AUTOSAVE_MINUTES" => value.parse::<u16>().ok().is_some_and(|n| {
            n.to_string() == value
                && if key == "AUTOSAVE_TURNS" {
                    (10..=1000).contains(&n)
                } else {
                    n <= 127
                }
        }),
        _ => false,
    }
}
