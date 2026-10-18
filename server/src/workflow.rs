//! hunks 和 build 的核心流程，命令行和 MCP 服务共用：
//! 收集改动并记下对比范围，再把 AI 写的步骤和最新的改动合成 walkthrough.json 与播放页面。

use crate::{
    diff::{self, CollectOptions, Diff, Hunk, OUT_DIR},
    model::{self, Report},
    page,
    util::{iso_now, to_posix, truthy, write_json, write_text},
};
use serde_json::{Map, Value, json};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

/// 改动块和对比范围，build 时按它重放
pub const HUNKS_FILE: &str = "hunks.json";
/// 给 AI 读的改动清单
pub const LISTING_FILE: &str = "hunks.md";
/// AI 写的讲解步骤
pub const STEPS_FILE: &str = "steps.json";
pub const WALKTHROUGH_FILE: &str = "walkthrough.json";
pub const PAGE_FILE: &str = "walkthrough.html";

/// 收集改动；有改动时把改动块和对比范围写进 `.onebyone/hunks.json`
pub fn collect(opts: &CollectOptions) -> Result<Diff, String> {
    let diff = diff::collect_diff(opts)?;
    if !diff.hunks.is_empty() {
        let out_dir = ensure_out_dir(&diff.root)?;
        write_json(
            &out_dir.join(HUNKS_FILE),
            &json!({
                "version": 1,
                "generatedAt": iso_now(),
                "base": diff.base,
                "head": diff.head,
                "paths": diff.paths,
                "files": diff.files,
                "hunks": diff.hunks,
            }),
        )?;
    }
    Ok(diff)
}

/// build 的结果
pub struct Built {
    pub walkthrough: Value,
    pub report: Report,
    /// 收集改动之后又新出现、消失的改动块数量；没有变化时为 None
    pub drift: Option<(usize, usize)>,
    pub json_path: PathBuf,
    pub page_path: PathBuf,
}

impl Built {
    /// 讲解覆盖了全部改动块，没有数据问题，代码也没有再变化
    pub fn is_clean(&self) -> bool {
        self.report.uncovered.is_empty() && self.report.issues.is_empty() && self.drift.is_none()
    }
}

/// 按 hunks.json 记下的范围重新收集改动（播放器里显示的永远是当前真实的代码），
/// 和步骤合成讲解，写出 walkthrough.json 和 walkthrough.html。
/// `steps_label` 用在错误提示里，说明步骤是从哪里来的。
pub fn build(
    root: &Path,
    saved: &Value,
    steps: &Value,
    steps_label: &str,
) -> Result<Built, String> {
    let paths: Vec<String> = saved
        .get("paths")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let diff = diff::collect_diff(&CollectOptions {
        cwd: root,
        base: saved
            .pointer("/base/rev")
            .and_then(Value::as_str)
            .unwrap_or("HEAD"),
        head: saved.pointer("/head/rev").and_then(Value::as_str),
        paths: &paths,
    })?;
    let invalid = |e: String| format!("{steps_label} 不符合格式：{e}");
    let walkthrough = assemble(steps, &diff).map_err(invalid)?;
    let report = model::check(&walkthrough).map_err(invalid)?;

    let out_dir = ensure_out_dir(&diff.root)?;
    let json_path = out_dir.join(WALKTHROUGH_FILE);
    write_json(&json_path, &walkthrough)?;
    let page_path = out_dir.join(PAGE_FILE);
    write_text(&page_path, &page::render(&walkthrough)?)?;

    let saved_ids: HashSet<&str> = saved
        .get("hunks")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|h| h.get("id").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let drift = compare_ids(&saved_ids, &diff.hunks);
    Ok(Built {
        walkthrough,
        report,
        drift,
        json_path,
        page_path,
    })
}

/// 步骤来自 AI，代码来自刚收集的 diff，两者合成播放器读取的 walkthrough.json
pub fn assemble(steps: &Value, diff: &Diff) -> Result<Value, String> {
    let Value::Object(s) = steps else {
        return Err("顶层必须是一个 JSON 对象".to_string());
    };
    let pick = |key: &str, fallback: &str| match s.get(key) {
        Some(v) if truthy(Some(v)) => v.clone(),
        _ => json!(fallback),
    };
    let base = match &diff.head {
        Some(head) => format!("{}..{}", diff.base.commit, head.commit),
        None => diff.base.commit.clone(),
    };
    let to_value = |v: serde_json::Result<Value>| v.map_err(|e| format!("生成 JSON 失败：{e}"));
    let mut w = Map::new();
    w.insert("version".into(), json!(1));
    w.insert("title".into(), pick("title", "未命名的改动"));
    w.insert("summary".into(), pick("summary", ""));
    w.insert("base".into(), json!(base));
    w.insert("generatedAt".into(), json!(iso_now()));
    w.insert("files".into(), to_value(serde_json::to_value(&diff.files))?);
    w.insert("hunks".into(), to_value(serde_json::to_value(&diff.hunks))?);
    if let Some(list) = s.get("steps") {
        w.insert("steps".into(), list.clone());
    }
    if let Some(timeline) = s.get("timeline").filter(|v| truthy(Some(v))) {
        w.insert("timeline".into(), timeline.clone());
    }
    Ok(Value::Object(w))
}

fn compare_ids(saved: &HashSet<&str>, fresh: &[Hunk]) -> Option<(usize, usize)> {
    let after: HashSet<&str> = fresh.iter().map(|h| h.id.as_str()).collect();
    let added = after.difference(saved).count();
    let removed = saved.difference(&after).count();
    (added > 0 || removed > 0).then_some((added, removed))
}

/// 目录里放一个只写 * 的 .gitignore：生成的文件不会出现在 git status 里，也不用改项目自己的 .gitignore
pub fn ensure_out_dir(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join(OUT_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 {} 失败：{e}", to_posix(&dir)))?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        write_text(&ignore, "*\n")?;
    }
    Ok(dir)
}

/// 对比范围的说明，例如 `HEAD（a1b2c3d） → 工作区（含未提交和未跟踪的文件）`
pub fn range_label(diff: &Diff) -> String {
    let target = diff
        .head
        .as_ref()
        .map_or("工作区（含未提交和未跟踪的文件）", |h| {
            h.label.as_str()
        });
    let scope = if diff.paths.is_empty() {
        String::new()
    } else {
        format!("，只看 {}", diff.paths.join("、"))
    };
    format!("{} → {target}{scope}", diff.base.label)
}
