//! 检查 walkthrough 数据，计算讲解覆盖率。
//!
//! 规则和播放器（web/src/core/model.ts）保持一致：build 报出的数据问题和覆盖率，
//! 与播放器左下角显示的一模一样。命令行只需要检查结果，所以这里不整理渲染用的结构。

use crate::util::{js_string, truthy};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

const STATUSES: [&str; 4] = ["added", "modified", "deleted", "renamed"];

/// 没有被任何讲解步骤引用的改动块
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uncovered {
    pub id: String,
    pub file: String,
    pub old_start: u64,
    pub old_count: u64,
    pub new_start: u64,
    pub new_count: u64,
    pub adds: u64,
    pub dels: u64,
}

#[derive(Debug, Default)]
pub struct Report {
    /// 讲解顺序里的步骤数（不含自动补上的「未讲解的改动」）
    pub step_count: usize,
    /// 最终 diff 里的改动块数
    pub total: usize,
    pub uncovered: Vec<Uncovered>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone)]
struct Hunk {
    id: String,
    file: String,
    old_start: u64,
    old_count: u64,
    new_start: u64,
    new_count: u64,
    final_: bool,
    adds: u64,
    dels: u64,
}

/// 检查数据；顶层结构不对（缺少 hunks 或 steps）时返回错误，其余问题记在 issues 里
pub fn check(raw: &Value) -> Result<Report, String> {
    let Value::Object(data) = raw else {
        return Err("顶层必须是一个 JSON 对象".to_string());
    };
    let Some(Value::Array(raw_hunks)) = data.get("hunks") else {
        return Err("缺少 hunks 数组".to_string());
    };
    let raw_steps = match data.get("steps") {
        Some(Value::Array(steps)) if !steps.is_empty() => steps,
        _ => return Err("缺少 steps 数组，或 steps 为空".to_string()),
    };

    let mut issues = Vec::new();
    if let Some(version) = data.get("version")
        && version.as_f64() != Some(1.0)
    {
        issues.push(format!(
            "数据版本是 {}，播放器按版本 1 解析",
            js_string(Some(version))
        ));
    }

    let empty = Map::new();
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    for (i, h) in raw_hunks.iter().enumerate() {
        let Some(obj) = as_object(h, &empty) else {
            issues.push(format!("hunks[{i}] 不是对象，已忽略"));
            continue;
        };
        let id = match obj.get("id") {
            None | Some(Value::Null) => String::new(),
            v => js_string(v),
        };
        if id.is_empty() {
            issues.push(format!("hunks[{i}] 缺少 id，已忽略"));
        } else if by_id.contains_key(&id) {
            issues.push(format!("改动块 id 重复：{id}，后出现的已忽略"));
        } else if !truthy(obj.get("file")) {
            issues.push(format!("改动块 {id} 缺少 file，已忽略"));
        } else if let Some(Value::Array(lines)) = obj.get("lines") {
            by_id.insert(id.clone(), hunks.len());
            hunks.push(parse_hunk(id, obj, lines));
        } else {
            issues.push(format!("改动块 {id} 缺少 lines 数组，已忽略"));
        }
    }

    if let Some(Value::Array(files)) = data.get("files") {
        for (i, f) in files.iter().enumerate() {
            let path = f.get("path");
            if !truthy(path) {
                issues.push(format!("files[{i}] 缺少 path，已忽略"));
                continue;
            }
            let status = if truthy(f.get("status")) {
                js_string(f.get("status"))
            } else {
                "modified".to_string()
            };
            if !STATUSES.contains(&status.as_str()) {
                let path = norm_path(&js_string(path));
                issues.push(format!(
                    "文件 {path} 的 status 无法识别：{status}，按“修改”处理"
                ));
            }
        }
    }

    let steps = build_track("讲解顺序", raw_steps, &by_id, &mut issues);
    match data.get("timeline") {
        Some(Value::Array(timeline)) if !timeline.is_empty() => {
            build_track("真实过程", timeline, &by_id, &mut issues);
        }
        None | Some(Value::Array(_)) => {}
        Some(_) => issues.push("timeline 不是数组，已忽略".to_string()),
    }
    for (i, step) in steps.iter().enumerate() {
        for &k in step {
            if !hunks[k].final_ {
                issues.push(format!(
                    "讲解顺序第 {} 步引用了过程中的改动块：{}",
                    i + 1,
                    hunks[k].id
                ));
            }
        }
    }

    // 覆盖率：最终 diff 里的每个改动块，都应该出现在某个讲解步骤里
    let covered: HashSet<usize> = steps.iter().flatten().copied().collect();
    let finals: Vec<&Hunk> = hunks.iter().filter(|h| h.final_).collect();
    let uncovered = hunks
        .iter()
        .enumerate()
        .filter(|(k, h)| h.final_ && !covered.contains(k))
        .map(|(_, h)| Uncovered {
            id: h.id.clone(),
            file: h.file.clone(),
            old_start: h.old_start,
            old_count: h.old_count,
            new_start: h.new_start,
            new_count: h.new_count,
            adds: h.adds,
            dels: h.dels,
        })
        .collect();

    Ok(Report {
        step_count: steps.len(),
        total: finals.len(),
        uncovered,
        issues,
    })
}

// JavaScript 里数组也是对象（`typeof [] === 'object'`），只是取不到任何字段
fn as_object<'a>(
    value: &'a Value,
    empty: &'a Map<String, Value>,
) -> Option<&'a Map<String, Value>> {
    match value {
        Value::Object(obj) => Some(obj),
        Value::Array(_) => Some(empty),
        _ => None,
    }
}

fn norm_path(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    p.strip_prefix("./").map(str::to_string).unwrap_or(p)
}

// 不是非负整数时按 1 处理（3.0 这样的写法也算整数）
fn line_start(value: Option<&Value>) -> u64 {
    value
        .and_then(|v| {
            v.as_u64().or_else(|| {
                v.as_f64()
                    .filter(|f| f.fract() == 0.0 && *f >= 0.0)
                    .map(|f| f as u64)
            })
        })
        .unwrap_or(1)
}

// 按 unified diff 的规则统计行数
fn parse_hunk(id: String, obj: &Map<String, Value>, lines: &[Value]) -> Hunk {
    let (mut old_count, mut new_count, mut adds, mut dels) = (0, 0, 0, 0);
    for line in lines {
        let text = if line.is_null() {
            String::new()
        } else {
            js_string(Some(line))
        };
        match text.chars().next() {
            Some('+') => {
                adds += 1;
                new_count += 1;
            }
            Some('-') => {
                dels += 1;
                old_count += 1;
            }
            Some('\\') => {}
            _ => {
                old_count += 1;
                new_count += 1;
            }
        }
    }
    Hunk {
        id,
        file: norm_path(&js_string(obj.get("file"))),
        old_start: line_start(obj.get("oldStart")),
        old_count,
        new_start: line_start(obj.get("newStart")),
        new_count,
        final_: obj.get("final") != Some(&Value::Bool(false)),
        adds,
        dels,
    }
}

/// 整理一条步骤列表，返回每一步引用的改动块（hunks 里的下标）
fn build_track(
    label: &str,
    list: &[Value],
    by_id: &HashMap<String, usize>,
    issues: &mut Vec<String>,
) -> Vec<Vec<usize>> {
    let empty = Map::new();
    let mut items: Vec<(String, Vec<usize>, Vec<Value>)> = Vec::new();
    for (i, s) in list.iter().enumerate() {
        let where_ = format!("{label}第 {} 步", i + 1);
        let Some(step) = as_object(s, &empty) else {
            issues.push(format!("{where_}不是对象，已忽略"));
            continue;
        };
        let mut refs: Vec<usize> = Vec::new();
        if let Some(Value::Array(list)) = step.get("hunks") {
            for r in list {
                let id = match r {
                    Value::Object(obj) => js_string(obj.get("id")),
                    Value::Array(_) => "undefined".to_string(),
                    other => js_string(Some(other)),
                };
                match by_id.get(&id) {
                    None => issues.push(format!("{where_}引用了不存在的改动块：{id}")),
                    Some(&k) if !refs.contains(&k) => refs.push(k),
                    Some(_) => {}
                }
            }
        }
        let id = match step.get("id") {
            None | Some(Value::Null) => String::new(),
            v => js_string(v),
        };
        let related = match step.get("related") {
            Some(Value::Array(list)) => list.clone(),
            _ => Vec::new(),
        };
        items.push((id, refs, related));
    }

    // related 可以写步骤 id，也可以写从 1 开始的序号
    for (i, (_, _, related)) in items.iter().enumerate() {
        for r in related {
            let k = match r {
                Value::Number(n) => n.as_f64().map(|f| f - 1.0),
                other => {
                    let target = js_string(Some(other));
                    items
                        .iter()
                        .position(|(id, _, _)| !id.is_empty() && *id == target)
                        .map(|k| k as f64)
                }
            };
            let valid = k.is_some_and(|k| {
                k.fract() == 0.0 && k >= 0.0 && k < items.len() as f64 && k != i as f64
            });
            if !valid {
                issues.push(format!(
                    "{label}第 {} 步的关联步骤无效：{}",
                    i + 1,
                    js_string(Some(r))
                ));
            }
        }
    }
    items.into_iter().map(|(_, refs, _)| refs).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn same_issues_as_the_player() {
        // 和 web/src/core/model.test.ts 里的用例一致
        let report = check(&json!({
            "version": 2,
            "hunks": [
                { "id": "h1", "file": "a.ts", "lines": ["+x"] },
                { "id": "h1", "file": "b.ts", "lines": [] },
                { "file": "c.ts", "lines": [] }
            ],
            "files": [{ "path": "a.ts", "status": "weird" }],
            "steps": [
                { "id": "one", "title": "一", "hunks": ["h1", "nope"], "related": ["two", 9] },
                { "id": "two", "title": "二", "related": [1] }
            ],
            "timeline": "x"
        }))
        .unwrap();
        assert_eq!(
            report.issues,
            vec![
                "数据版本是 2，播放器按版本 1 解析",
                "改动块 id 重复：h1，后出现的已忽略",
                "hunks[2] 缺少 id，已忽略",
                "文件 a.ts 的 status 无法识别：weird，按“修改”处理",
                "讲解顺序第 1 步引用了不存在的改动块：nope",
                "讲解顺序第 1 步的关联步骤无效：9",
                "timeline 不是数组，已忽略",
            ]
        );
        assert_eq!((report.step_count, report.total), (2, 1));
        assert!(report.uncovered.is_empty());
    }

    #[test]
    fn uncovered_hunks_with_line_counts() {
        let report = check(&json!({
            "hunks": [
                { "id": "h1", "file": "./src\\a.ts", "oldStart": 3, "newStart": 3, "lines": [" a", "-b", "+c", "+d"] },
                { "id": "h2", "file": "b.ts", "lines": ["+x"], "final": false },
                { "id": "h3", "file": "c.ts", "lines": ["+y"] }
            ],
            "steps": [{ "title": "只讲 c", "hunks": ["h3", "h2"] }]
        }))
        .unwrap();
        assert_eq!(
            report.issues,
            vec!["讲解顺序第 1 步引用了过程中的改动块：h2"]
        );
        assert_eq!(report.total, 2);
        assert_eq!(
            report.uncovered,
            vec![Uncovered {
                id: "h1".into(),
                file: "src/a.ts".into(),
                old_start: 3,
                old_count: 2,
                new_start: 3,
                new_count: 3,
                adds: 2,
                dels: 1
            }]
        );
    }

    #[test]
    fn broken_top_level_is_an_error() {
        assert_eq!(check(&json!([])).unwrap_err(), "顶层必须是一个 JSON 对象");
        assert_eq!(
            check(&json!({ "steps": [{}] })).unwrap_err(),
            "缺少 hunks 数组"
        );
        assert_eq!(
            check(&json!({ "hunks": [], "steps": [] })).unwrap_err(),
            "缺少 steps 数组，或 steps 为空"
        );
    }
}
