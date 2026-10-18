//! 命令行用到的小工具：路径显示、JSON 读写、时间格式化，以及按 JavaScript 的规则理解 JSON 值。

use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// 路径统一用 `/` 分隔，Windows 上也一样
pub fn to_posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// 显示路径时尽量用相对 `base` 的写法，不在 `base` 里时用完整路径
pub fn display(path: &Path, base: &Path) -> String {
    match path.strip_prefix(base) {
        Ok(rel) if !rel.as_os_str().is_empty() => to_posix(rel),
        _ => to_posix(path),
    }
}

/// 把 `a/./b/../c` 这样的路径整理成 `a/c`；结果为空时返回 `.`
pub fn normalize_posix(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

pub fn read_json(file: &Path, base: &Path) -> Result<Value, String> {
    let text =
        fs::read_to_string(file).map_err(|e| format!("读取 {} 失败：{e}", display(file, base)))?;
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("{} 不是合法的 JSON：{e}", display(file, base)))
}

pub fn write_json<T: Serialize>(file: &Path, data: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(data).map_err(|e| format!("生成 JSON 失败：{e}"))?;
    write_text(file, &format!("{text}\n"))
}

pub fn write_text(file: &Path, text: &str) -> Result<(), String> {
    fs::write(file, text).map_err(|e| format!("写入 {} 失败：{e}", to_posix(file)))
}

/// 当前时间，格式和 JavaScript 的 `toISOString()` 一样，例如 `2026-10-18T09:30:00.000Z`
pub fn iso_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    iso_from_millis(now.as_millis() as i64)
}

pub fn iso_from_millis(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        ms.rem_euclid(1000)
    )
}

// 从 1970-01-01 起的天数换算成年月日（Howard Hinnant 的公历算法）
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

// ---------- 按 JavaScript 的规则理解 JSON 值 ----------
// 讲解数据由 AI 手写，播放器（TypeScript）按 JavaScript 的规则读取。
// 命令行的检查要和播放器一致，所以这里模拟 JavaScript 的真假判断和字符串转换。

/// JavaScript 里的真值判断：`null`、`false`、`0`、空字符串和缺失都算假
pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_) | Value::Object(_)) => true,
    }
}

/// JavaScript 的 `String(value)`；`None` 相当于 `undefined`
pub fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => js_number(n),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                if v.is_null() {
                    String::new()
                } else {
                    js_string(Some(v))
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".to_string(),
    }
}

fn js_number(n: &serde_json::Number) -> String {
    match n.as_f64() {
        Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", f as i64),
        Some(f) => f.to_string(),
        None => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn iso_time_matches_javascript() {
        assert_eq!(iso_from_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            iso_from_millis(1_792_311_045_123),
            "2026-10-18T08:10:45.123Z"
        );
        assert_eq!(iso_from_millis(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn normalize_relative_paths() {
        assert_eq!(normalize_posix("web/./src/../dist"), "web/dist");
        assert_eq!(normalize_posix(""), ".");
        assert_eq!(normalize_posix("../other"), "../other");
    }

    #[test]
    fn javascript_truthiness_and_strings() {
        assert!(!truthy(Some(&json!(""))));
        assert!(!truthy(Some(&json!(0))));
        assert!(truthy(Some(&json!([]))));
        assert_eq!(js_string(Some(&json!(3.0))), "3");
        assert_eq!(js_string(Some(&json!(1.5))), "1.5");
        assert_eq!(js_string(Some(&json!(["a", null, 2]))), "a,,2");
        assert_eq!(js_string(Some(&json!({ "a": 1 }))), "[object Object]");
        assert_eq!(js_string(None), "undefined");
    }
}
