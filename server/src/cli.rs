//! 命令行入口：hunks / build / open / install-skill。
//!
//! 退出码：0 成功；1 出错（不是 git 仓库、JSON 写错等）；2 已生成，但还有没讲到的改动块或数据问题。

use crate::{
    diff::{self, CollectOptions, Diff, Hunk, OUT_DIR},
    listing,
    model::{self, Report},
    page,
    util::{display, home_dir, iso_now, read_json, to_posix, truthy, write_json, write_text},
};
use serde_json::{Map, Value, json};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

const HELP: &str = "用法：
  onebyone hunks [--base <提交>] [--head <提交>] [-- <路径>...]
      把改动切成改动块，写出 .onebyone/hunks.md（给 AI 读的清单）
      默认对比 HEAD 和工作区（含未提交和未跟踪的文件）
  onebyone build
      读取 .onebyone/steps.json，检查覆盖率，生成 walkthrough.json 和播放页面 walkthrough.html
  onebyone open [<walkthrough.json>]
      用浏览器打开 build 生成的播放页面；指定 JSON 文件时先用它生成页面
  onebyone install-skill [--project <项目目录>]
      安装 Claude Code 的 /walkthrough skill（默认装到 ~/.claude/skills）

退出码：0 成功；1 出错；2 已生成，但还有未讲解的改动块或数据问题";

const SKILL: &str = include_str!("../assets/SKILL.md");

enum CliError {
    /// 用法写错，同时显示帮助
    Usage(String),
    Failed(String),
}

impl From<String> for CliError {
    fn from(msg: String) -> Self {
        CliError::Failed(msg)
    }
}

type CliResult = Result<i32, CliError>;

#[derive(Default)]
struct Options {
    base: Option<String>,
    head: Option<String>,
    project: Option<String>,
    paths: Vec<String>,
    positional: Vec<String>,
}

pub fn run(args: &[String]) -> i32 {
    let Some(name) = args.first() else {
        println!("{HELP}");
        return 0;
    };
    let rest = &args[1..];
    let result = match name.as_str() {
        "help" | "-h" | "--help" => {
            println!("{HELP}");
            return 0;
        }
        "-V" | "--version" => {
            println!("onebyone {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        "hunks" => parse_options(rest, &["base", "head"], true, 0).and_then(|o| cmd_hunks(&o)),
        "build" => parse_options(rest, &[], false, 0).and_then(|_| cmd_build()),
        "open" => parse_options(rest, &[], false, 1).and_then(|o| cmd_open(&o)),
        "install-skill" => {
            parse_options(rest, &["project"], false, 0).and_then(|o| cmd_install_skill(&o))
        }
        other => Err(CliError::Usage(format!("未知命令：{other}"))),
    };
    match result {
        Ok(code) => code,
        Err(CliError::Usage(msg)) => {
            eprintln!("错误：{msg}\n\n{HELP}");
            1
        }
        Err(CliError::Failed(msg)) => {
            eprintln!("错误：{msg}");
            1
        }
    }
}

fn parse_options(
    args: &[String],
    keys: &[&str],
    allow_paths: bool,
    max_positional: usize,
) -> Result<Options, CliError> {
    let mut opts = Options::default();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" && allow_paths {
            opts.paths = args[i + 1..].to_vec();
            break;
        }
        match arg.strip_prefix("--") {
            Some(key) if keys.contains(&key) => {
                let value = args
                    .get(i + 1)
                    .filter(|v| !v.is_empty() && !v.starts_with("--"))
                    .ok_or_else(|| CliError::Usage(format!("{arg} 后面需要一个值")))?
                    .clone();
                match key {
                    "base" => opts.base = Some(value),
                    "head" => opts.head = Some(value),
                    _ => opts.project = Some(value),
                }
                i += 2;
            }
            None if opts.positional.len() < max_positional => {
                opts.positional.push(arg.clone());
                i += 1;
            }
            _ => return Err(CliError::Usage(format!("不支持的参数：{arg}"))),
        }
    }
    Ok(opts)
}

// ---------- hunks：切分改动 ----------

fn cmd_hunks(opts: &Options) -> CliResult {
    let cwd = current_dir()?;
    let diff = diff::collect_diff(&CollectOptions {
        cwd: &cwd,
        base: opts.base.as_deref().unwrap_or("HEAD"),
        head: opts.head.as_deref(),
        paths: &opts.paths,
    })?;
    let range = range_label(&diff);
    if diff.hunks.is_empty() {
        println!("没有发现改动（{range}）。");
        return Ok(0);
    }
    let cli = cli_command();
    let out_dir = ensure_out_dir(&diff.root)?;
    write_json(
        &out_dir.join("hunks.json"),
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
    let listing_path = out_dir.join("hunks.md");
    write_text(&listing_path, &listing::render(&diff, &range, &cli))?;

    let (adds, dels) = listing::count_lines(&diff.hunks);
    println!(
        "已切分：{} 个文件，{} 个改动块（+{adds} −{dels}）",
        diff.files.len(),
        diff.hunks.len()
    );
    println!("对比范围：{range}");
    println!("改动清单：{}", display(&listing_path, &cwd));
    // 之前写过步骤时，顺便说明哪些引用失效、哪些还没讲，方便在原来的基础上修改
    let steps_file = out_dir.join("steps.json");
    if steps_file.exists() {
        println!(
            "已有 steps.json：{}",
            check_existing_steps(&steps_file, &diff, &cwd)
        );
    }
    println!();
    println!("下一步：按清单末尾的「写法要求」写 .onebyone/steps.json，然后运行：");
    println!("  {cli} build");
    Ok(0)
}

fn check_existing_steps(steps_file: &Path, diff: &Diff, cwd: &Path) -> String {
    let report = read_json(steps_file, cwd)
        .and_then(|steps| assemble(&steps, diff))
        .and_then(|w| model::check(&w));
    match report {
        Ok(r) => {
            let broken = r
                .issues
                .iter()
                .filter(|msg| msg.contains("不存在的改动块"))
                .count();
            format!(
                "{broken} 处引用已失效，{} 个改动块还没讲到",
                r.uncovered.len()
            )
        }
        Err(e) => format!("无法解析（{e}）"),
    }
}

// ---------- build：合并步骤、检查覆盖率、生成播放页面 ----------

fn cmd_build() -> CliResult {
    let cwd = current_dir()?;
    let root = diff::repo_root(&cwd)?;
    let cli = cli_command();
    let out_dir = root.join(OUT_DIR);
    let hunks_file = out_dir.join("hunks.json");
    let steps_file = out_dir.join("steps.json");
    if !hunks_file.exists() {
        return Err(format!(
            "找不到 {}，请先运行：{cli} hunks",
            display(&hunks_file, &cwd)
        )
        .into());
    }
    if !steps_file.exists() {
        let listing_path = out_dir.join("hunks.md");
        return Err(format!(
            "找不到 {}，请先按 {} 末尾的要求写好步骤",
            display(&steps_file, &cwd),
            display(&listing_path, &cwd)
        )
        .into());
    }

    let saved = read_json(&hunks_file, &cwd)?;
    let paths: Vec<String> = saved
        .get("paths")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    // 重新收集一次改动：播放器里显示的永远是当前真实的代码
    let diff = diff::collect_diff(&CollectOptions {
        cwd: &root,
        base: saved
            .pointer("/base/rev")
            .and_then(Value::as_str)
            .unwrap_or("HEAD"),
        head: saved.pointer("/head/rev").and_then(Value::as_str),
        paths: &paths,
    })?;
    let walkthrough = assemble(&read_json(&steps_file, &cwd)?, &diff)?;
    let report = model::check(&walkthrough).map_err(|e| format!("steps.json 不符合格式：{e}"))?;
    let output = out_dir.join("walkthrough.json");
    write_json(&output, &walkthrough)?;
    let page_path = out_dir.join("walkthrough.html");
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
    let ok = print_report(&report, compare_ids(&saved_ids, &diff.hunks), &cli);
    println!();
    println!("已生成：{}", display(&output, &cwd));
    println!("播放页面：{}", display(&page_path, &cwd));
    println!("在浏览器里打开：{cli} open");
    Ok(if ok { 0 } else { 2 })
}

// 步骤来自 AI，代码来自刚收集的 diff，两者合成播放器读取的 walkthrough.json
fn assemble(steps: &Value, diff: &Diff) -> Result<Value, String> {
    let Value::Object(s) = steps else {
        return Err("steps.json 顶层必须是一个 JSON 对象".to_string());
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

fn print_report(report: &Report, drift: Option<(usize, usize)>, cli: &str) -> bool {
    let covered = report.total - report.uncovered.len();
    println!("讲解步骤：{} 个", report.step_count);
    if report.uncovered.is_empty() {
        println!("讲解覆盖：{covered}/{}，讲解覆盖了全部改动块", report.total);
    } else {
        println!(
            "讲解覆盖：{covered}/{}，还有 {} 个改动块没有讲到：",
            report.total,
            report.uncovered.len()
        );
        for h in &report.uncovered {
            let label = listing::hunk_label(h.old_start, h.old_count, h.new_start, h.new_count);
            println!("  {}  {}  {label}  +{} −{}", h.id, h.file, h.adds, h.dels);
        }
    }
    if !report.issues.is_empty() {
        println!("数据问题 {} 条：", report.issues.len());
        for msg in &report.issues {
            println!("  - {msg}");
        }
    }
    if let Some((added, removed)) = drift {
        println!(
            "注意：运行 hunks 之后改动又有变化（新出现 {added} 个、消失 {removed} 个改动块）。"
        );
        println!("请重新运行 {cli} hunks，按新的清单更新 steps.json。");
    }
    report.uncovered.is_empty() && report.issues.is_empty() && drift.is_none()
}

// ---------- open：在浏览器里打开播放页面 ----------

fn cmd_open(opts: &Options) -> CliResult {
    let cwd = current_dir()?;
    let page_path = match opts.positional.first() {
        None => {
            let root = diff::repo_root(&cwd)?;
            let page_path = root.join(OUT_DIR).join("walkthrough.html");
            if !page_path.exists() {
                return Err(format!(
                    "找不到 {}，请先运行：{} build",
                    display(&page_path, &cwd),
                    cli_command()
                )
                .into());
            }
            page_path
        }
        Some(file) => {
            let json_path = cwd.join(file);
            let data = read_json(&json_path, &cwd)?;
            model::check(&data)
                .map_err(|e| format!("{} 不符合格式：{e}", display(&json_path, &cwd)))?;
            let page_path = temp_page_path(&json_path)?;
            write_text(&page_path, &page::render(&data)?)?;
            page_path
        }
    };
    println!("播放页面：{}", display(&page_path, &cwd));
    // 打不开浏览器（例如没有图形界面）时，页面已经生成好了，手动打开即可
    if let Err(e) = page::open_in_browser(&page_path) {
        eprintln!("{e}，可以手动打开上面的文件");
    }
    Ok(0)
}

// 用任意 JSON 生成的页面放在系统临时目录，不在项目里留下文件
fn temp_page_path(json_path: &Path) -> Result<PathBuf, String> {
    let dir = env::temp_dir().join("onebyone");
    fs::create_dir_all(&dir).map_err(|e| format!("创建 {} 失败：{e}", to_posix(&dir)))?;
    let stem = json_path.file_stem().map_or_else(
        || "walkthrough".into(),
        |s| s.to_string_lossy().into_owned(),
    );
    let mut sha = sha1_smol::Sha1::new();
    sha.update(to_posix(json_path).as_bytes());
    let tag = sha.digest().to_string();
    Ok(dir.join(format!("{stem}-{}.html", &tag[..8])))
}

// ---------- install-skill ----------

fn cmd_install_skill(opts: &Options) -> CliResult {
    let cwd = current_dir()?;
    let skills_dir = match &opts.project {
        Some(project) => cwd.join(project).join(".claude").join("skills"),
        None => home_dir()?.join(".claude").join("skills"),
    };
    let target = skills_dir.join("walkthrough").join("SKILL.md");
    let existed = target.exists();
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 {} 失败：{e}", to_posix(parent)))?;
    }
    write_text(&target, &SKILL.replace("{{ONEBYONE}}", &cli_command()))?;
    println!(
        "{}：{}",
        if existed { "已更新" } else { "已安装" },
        target.display()
    );
    println!("在 Claude Code 里输入 /walkthrough 即可使用，也可以直接说\"讲解一下这次的改动\"。");
    Ok(0)
}

// ---------- 小工具 ----------

fn current_dir() -> Result<PathBuf, String> {
    env::current_dir().map_err(|e| format!("读取当前目录失败：{e}"))
}

/// 提示里使用的命令：当前程序的完整路径，路径含空格时加引号
fn cli_command() -> String {
    let exe = env::current_exe()
        .map(|p| to_posix(&p))
        .unwrap_or_else(|_| "onebyone".to_string());
    let exe = exe.strip_prefix("//?/").map(str::to_string).unwrap_or(exe);
    if exe.contains(char::is_whitespace) {
        format!("\"{exe}\"")
    } else {
        exe
    }
}

// 目录里放一个只写 * 的 .gitignore：生成的文件不会出现在 git status 里，也不用改项目自己的 .gitignore
fn ensure_out_dir(root: &Path) -> Result<PathBuf, String> {
    let dir = root.join(OUT_DIR);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 {} 失败：{e}", to_posix(&dir)))?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        write_text(&ignore, "*\n")?;
    }
    Ok(dir)
}

fn range_label(diff: &Diff) -> String {
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
