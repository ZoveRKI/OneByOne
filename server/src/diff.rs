//! 收集并解析 git 改动，切成带稳定 id 的改动块（hunk）。
//!
//! 改动块的 id 由「文件路径 + 改动内容」算出来（例如 `h3f2a9c`）：
//! 内容不变 id 就不变；内容变了 id 也跟着变，旧的步骤引用会对不上，build 能直接发现。

use crate::util::{normalize_posix, to_posix};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
};

pub const OUT_DIR: &str = ".onebyone";
// git 的空树对象：仓库还没有任何提交时，用它作为对比基准
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
// 未跟踪的新文件超过这个大小时只记录"文件过大"，不读内容
const MAX_UNTRACKED_BYTES: u64 = 2 * 1024 * 1024;
// 判断二进制文件时检查的开头字节数，和 git 的做法一致
const BINARY_SNIFF_BYTES: usize = 8000;

/// 对比的一端
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevInfo {
    pub rev: String,
    /// 短提交号；空仓库时为空
    pub commit: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
}

/// 写进 hunks.json / walkthrough.json 的改动块
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub id: String,
    pub file: String,
    pub old_start: u64,
    pub new_start: u64,
    pub lines: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub header: String,
}

#[derive(Debug)]
pub struct Diff {
    pub root: PathBuf,
    pub base: RevInfo,
    pub head: Option<RevInfo>,
    /// 只看这些路径（相对仓库根目录）；为空表示整个仓库
    pub paths: Vec<String>,
    pub files: Vec<FileEntry>,
    pub hunks: Vec<Hunk>,
}

/// 解析 unified diff 得到的单个文件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub status: &'static str,
    pub binary: bool,
    pub old_mode: String,
    pub new_mode: String,
    pub note: String,
    pub hunks: Vec<RawHunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawHunk {
    pub old_start: u64,
    pub old_count: u64,
    pub new_start: u64,
    pub new_count: u64,
    pub header: String,
    pub lines: Vec<String>,
}

impl FileDiff {
    fn new(old_path: Option<String>, new_path: Option<String>) -> Self {
        Self {
            old_path,
            new_path,
            status: "modified",
            binary: false,
            old_mode: String::new(),
            new_mode: String::new(),
            note: String::new(),
            hunks: Vec::new(),
        }
    }
}

// ---------- 调用 git ----------

fn git_command(args: &[&str], cwd: &Path) -> std::io::Result<std::process::Output> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(cwd);
    #[cfg(windows)]
    {
        // 从没有控制台的程序（例如 MCP 客户端）里调用时，不弹出命令行窗口
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output()
}

fn not_found(err: &std::io::Error) -> String {
    if err.kind() == ErrorKind::NotFound {
        "找不到 git 命令，请先安装 Git".to_string()
    } else {
        err.to_string()
    }
}

/// 运行 git 并返回标准输出；失败时返回 git 的错误信息
pub fn git(args: &[&str], cwd: &Path) -> Result<String, String> {
    let out = git_command(args, cwd).map_err(|e| not_found(&e))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git {} 执行失败", args.join(" "))
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn repo_root(cwd: &Path) -> Result<PathBuf, String> {
    let out = git_command(&["rev-parse", "--show-toplevel"], cwd).map_err(|e| not_found(&e))?;
    if !out.status.success() {
        return Err(format!("{} 不在 git 仓库里", cwd.display()));
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

fn rev_exists(rev: &str, cwd: &Path) -> bool {
    let spec = format!("{rev}^{{commit}}");
    git_command(&["rev-parse", "--verify", "--quiet", &spec], cwd).is_ok_and(|o| o.status.success())
}

fn describe_rev(rev: &str, root: &Path) -> Result<RevInfo, String> {
    if rev == EMPTY_TREE {
        return Ok(RevInfo {
            rev: rev.to_string(),
            commit: String::new(),
            label: "空仓库（还没有任何提交）".to_string(),
        });
    }
    if !rev_exists(rev, root) {
        return Err(format!("找不到提交：{rev}"));
    }
    let commit = git(
        &["rev-parse", "--short", &format!("{rev}^{{commit}}")],
        root,
    )?
    .trim()
    .to_string();
    let label = if rev == commit {
        commit.clone()
    } else {
        format!("{rev}（{commit}）")
    };
    Ok(RevInfo {
        rev: rev.to_string(),
        commit,
        label,
    })
}

// ---------- 收集改动 ----------

pub struct CollectOptions<'a> {
    /// 在哪个目录下找仓库
    pub cwd: &'a Path,
    /// 对比基准，通常是 HEAD
    pub base: &'a str,
    /// 不填表示对比到工作区（含未提交和未跟踪的文件）
    pub head: Option<&'a str>,
    /// 只看这些路径（相对 cwd）
    pub paths: &'a [String],
}

/// 收集改动并切成改动块
pub fn collect_diff(opts: &CollectOptions) -> Result<Diff, String> {
    let root = repo_root(opts.cwd)?;
    let base_rev = if opts.base == "HEAD" && !rev_exists("HEAD", &root) {
        EMPTY_TREE
    } else {
        opts.base
    };
    let base = describe_rev(base_rev, &root)?;
    let head = opts.head.map(|h| describe_rev(h, &root)).transpose()?;
    // 路径统一换成相对仓库根目录，build 时可以在任意子目录里重放
    let paths = root_relative_paths(opts.cwd, &root, opts.paths)?;
    let mut specs: Vec<String> = if paths.is_empty() {
        vec![".".to_string()]
    } else {
        paths.clone()
    };
    specs.push(format!(":(exclude){OUT_DIR}"));

    let mut args = vec![
        "-c",
        "core.quotepath=off",
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--find-renames",
        "--unified=3",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        base_rev,
    ];
    if let Some(h) = opts.head {
        args.push(h);
    }
    args.push("--");
    args.extend(specs.iter().map(String::as_str));
    let mut file_diffs = parse_unified_diff(&git(&args, &root)?);
    if opts.head.is_none() {
        file_diffs.extend(collect_untracked(&root, &specs)?);
    }
    let (files, hunks) = to_hunks(&file_diffs);
    Ok(Diff {
        root,
        base,
        head,
        paths,
        files,
        hunks,
    })
}

fn root_relative_paths(cwd: &Path, root: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    // 当前目录相对仓库根目录的前缀，例如 "web/"；直接问 git，避免自己比较 Windows 路径的大小写和分隔符
    let prefix = git(&["rev-parse", "--show-prefix"], cwd)?
        .trim()
        .to_string();
    Ok(paths
        .iter()
        .map(|p| {
            let path = Path::new(p);
            if path.is_absolute() {
                match path.strip_prefix(root) {
                    Ok(rel) => normalize_posix(&to_posix(rel)),
                    Err(_) => to_posix(path),
                }
            } else {
                normalize_posix(&format!("{prefix}{}", p.replace('\\', "/")))
            }
        })
        .collect())
}

// git diff 不包含未跟踪的新文件（AI 新建、还没 git add 的文件），这里单独读取
fn collect_untracked(root: &Path, specs: &[String]) -> Result<Vec<FileDiff>, String> {
    let mut args = vec!["ls-files", "--others", "--exclude-standard", "-z", "--"];
    args.extend(specs.iter().map(String::as_str));
    let list = git(&args, root)?;
    Ok(list
        .split('\0')
        .filter(|p| !p.is_empty() && !p.ends_with('/'))
        .map(|rel| read_untracked(root, rel))
        .collect())
}

fn read_untracked(root: &Path, rel: &str) -> FileDiff {
    let mut file = FileDiff::new(None, Some(rel.to_string()));
    file.status = "added";
    let full = root.join(rel);
    let size = match fs::metadata(&full) {
        Ok(meta) => meta.len(),
        Err(err) => {
            file.note = format!("无法读取这个新文件：{err}");
            return file;
        }
    };
    if size > MAX_UNTRACKED_BYTES {
        file.note = format!(
            "新文件较大（{:.1} MB），没有读取内容",
            size as f64 / 1024.0 / 1024.0
        );
        return file;
    }
    let buf = match fs::read(&full) {
        Ok(buf) => buf,
        Err(err) => {
            file.note = format!("无法读取这个新文件：{err}");
            return file;
        }
    };
    if buf.iter().take(BINARY_SNIFF_BYTES).any(|b| *b == 0) {
        file.binary = true;
        return file;
    }
    let text = String::from_utf8_lossy(&buf);
    if text.is_empty() {
        return file;
    }
    let mut lines = split_lines(&text);
    let ends_with_newline = lines.last().is_some_and(|l| l.is_empty());
    if ends_with_newline {
        lines.pop();
    }
    let new_count = lines.len() as u64;
    let mut hunk_lines: Vec<String> = lines.into_iter().map(|l| format!("+{l}")).collect();
    if !ends_with_newline {
        hunk_lines.push("\\ No newline at end of file".to_string());
    }
    file.hunks.push(RawHunk {
        old_start: 0,
        old_count: 0,
        new_start: 1,
        new_count,
        header: String::new(),
        lines: hunk_lines,
    });
    file
}

// 按 \r?\n 切行：只有后面紧跟 \n 的 \r 才算行尾
fn split_lines(text: &str) -> Vec<&str> {
    let parts: Vec<&str> = text.split('\n').collect();
    let last = parts.len() - 1;
    parts
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            if i < last {
                p.strip_suffix('\r').unwrap_or(p)
            } else {
                p
            }
        })
        .collect()
}

// ---------- 解析 unified diff ----------

fn strip_cr(s: &str) -> &str {
    s.strip_suffix('\r').unwrap_or(s)
}

/// 把 `git diff` 的输出解析成按文件分组的改动块
pub fn parse_unified_diff(text: &str) -> Vec<FileDiff> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut files: Vec<FileDiff> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = strip_cr(lines[i]);
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let (a, b) = parse_git_line_paths(rest);
            files.push(FileDiff::new(Some(a), Some(b)));
            i += 1;
        } else if line.starts_with("@@ ") && !files.is_empty() {
            let file = files.last_mut().expect("前面已经判断过非空");
            i = read_hunk(&lines, i, file);
        } else {
            if let Some(file) = files.last_mut() {
                read_header_line(line, file);
            }
            i += 1;
        }
    }
    files
}

// "@@ -1,3 +1,4 @@ 上下文"：行数省略时表示 1
fn parse_hunk_header(line: &str) -> Option<(u64, u64, u64, u64, String)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, rest) = rest.split_once(" @@")?;
    let range = |r: &str| -> Option<(u64, u64)> {
        match r.split_once(',') {
            Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
            None => Some((r.parse().ok()?, 1)),
        }
    };
    let (old_start, old_count) = range(old)?;
    let (new_start, new_count) = range(new)?;
    let header = rest.strip_prefix(' ').unwrap_or(rest).to_string();
    Some((old_start, old_count, new_start, new_count, header))
}

// 按 @@ 里给出的行数读取，不靠行首字符判断结束，避免把内容里的 "--- " 误当成文件头
fn read_hunk(lines: &[&str], start: usize, file: &mut FileDiff) -> usize {
    let Some((old_start, old_count, new_start, new_count, header)) =
        parse_hunk_header(strip_cr(lines[start]))
    else {
        return start + 1;
    };
    let mut hunk = RawHunk {
        old_start,
        old_count,
        new_start,
        new_count,
        header,
        lines: Vec::new(),
    };
    let (mut old_left, mut new_left) = (old_count, new_count);
    let mut i = start + 1;
    while i < lines.len() {
        let raw = strip_cr(lines[i]);
        if raw.starts_with('\\') {
            hunk.lines.push(raw.to_string());
            i += 1;
            continue;
        }
        if old_left == 0 && new_left == 0 {
            break;
        }
        match raw.chars().next() {
            Some('+') => new_left = new_left.saturating_sub(1),
            Some('-') => old_left = old_left.saturating_sub(1),
            Some(' ') | None => {
                old_left = old_left.saturating_sub(1);
                new_left = new_left.saturating_sub(1);
            }
            _ => break, // 行数对不上，说明 diff 不完整，停在这里
        }
        hunk.lines.push(if raw.is_empty() {
            " ".to_string()
        } else {
            raw.to_string()
        });
        i += 1;
    }
    file.hunks.push(hunk);
    i
}

fn read_header_line(line: &str, file: &mut FileDiff) {
    if let Some(v) = line.strip_prefix("--- ") {
        match header_path(v, "a/") {
            None => file.status = "added",
            Some(p) => file.old_path = Some(p),
        }
    } else if let Some(v) = line.strip_prefix("+++ ") {
        match header_path(v, "b/") {
            None => file.status = "deleted",
            Some(p) => file.new_path = Some(p),
        }
    } else if let Some(v) = line.strip_prefix("new file mode ") {
        file.status = "added";
        file.new_mode = v.to_string();
    } else if let Some(v) = line.strip_prefix("deleted file mode ") {
        file.status = "deleted";
        file.old_mode = v.to_string();
    } else if let Some(v) = line.strip_prefix("old mode ") {
        file.old_mode = v.to_string();
    } else if let Some(v) = line.strip_prefix("new mode ") {
        file.new_mode = v.to_string();
    } else if let Some(v) = line.strip_prefix("rename from ") {
        file.status = "renamed";
        file.old_path = Some(unquote(v));
    } else if let Some(v) = line.strip_prefix("rename to ") {
        file.status = "renamed";
        file.new_path = Some(unquote(v));
    } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
        file.binary = true;
    }
}

// "--- a/路径"：路径含空格时 git 会在末尾加一个制表符
fn header_path(value: &str, prefix: &str) -> Option<String> {
    let s = value.strip_suffix('\t').unwrap_or(value);
    if s == "/dev/null" {
        return None;
    }
    let p = unquote(s);
    Some(p.strip_prefix(prefix).map(str::to_string).unwrap_or(p))
}

// "diff --git a/x b/x"：路径相同时正好从中间切开，路径里有空格也没问题
fn parse_git_line_paths(rest: &str) -> (String, String) {
    let strip = |p: String, prefix: &str| p.strip_prefix(prefix).map(str::to_string).unwrap_or(p);
    if rest.starts_with('"')
        && let Some(end) = quoted_end(rest)
        && let Some(second) = rest[end..].strip_prefix(' ')
        && !second.is_empty()
    {
        return (
            strip(unquote(&rest[..end]), "a/"),
            strip(unquote(second), "b/"),
        );
    }
    if rest.len() % 2 == 1 {
        let half = (rest.len() - 1) / 2;
        if rest.is_char_boundary(half) && rest[half..].starts_with(' ') {
            let (a, b) = (&rest[..half], &rest[half + 1..]);
            if let (Some(a), Some(b)) = (a.strip_prefix("a/"), b.strip_prefix("b/"))
                && a == b
            {
                return (a.to_string(), b.to_string());
            }
        }
    }
    if let Some(after) = rest.strip_prefix("a/")
        && let Some(pos) = after.find(" b/")
    {
        return (after[..pos].to_string(), after[pos + 3..].to_string());
    }
    (rest.to_string(), rest.to_string())
}

// 带引号的路径在哪里结束（返回结束引号之后的位置）
fn quoted_end(s: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in s.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// 还原 git 的 C 风格引号路径，例如 `"a/\346\226\207.txt"`
pub fn unquote(s: &str) -> String {
    if !(s.len() >= 2 && s.starts_with('"') && s.ends_with('"')) {
        return s.to_string();
    }
    let inner = &s[1..s.len() - 1];
    let mut bytes: Vec<u8> = Vec::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        let Some(next) = chars.next() else {
            bytes.push(b'\\');
            break;
        };
        // 三位八进制数表示一个字节，连续几个字节组成一个 UTF-8 字符
        if next.is_digit(8) {
            let mut digits = next.to_string();
            while digits.len() < 3 && chars.peek().is_some_and(|d| d.is_digit(8)) {
                digits.push(chars.next().unwrap_or('0'));
            }
            if digits.len() == 3
                && let Ok(byte) = u8::from_str_radix(&digits, 8)
            {
                bytes.push(byte);
                continue;
            }
            bytes.extend_from_slice(digits.as_bytes());
            continue;
        }
        let escaped = match next {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'a' => '\x07',
            'b' => '\x08',
            'f' => '\x0c',
            'v' => '\x0b',
            other => other,
        };
        let mut buf = [0u8; 4];
        bytes.extend_from_slice(escaped.encode_utf8(&mut buf).as_bytes());
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------- 整理成改动块 ----------

fn to_hunks(file_diffs: &[FileDiff]) -> (Vec<FileEntry>, Vec<Hunk>) {
    let mut files = Vec::new();
    let mut hunks = Vec::new();
    let mut used_ids = HashSet::new();
    for f in file_diffs {
        let path = if f.status == "deleted" {
            f.old_path.clone()
        } else {
            f.new_path.clone()
        }
        .unwrap_or_default();
        let old_path = if f.status == "renamed" {
            f.old_path.clone()
        } else {
            None
        };
        files.push(FileEntry {
            path: path.clone(),
            status: f.status.to_string(),
            old_path,
        });
        for h in &f.hunks {
            hunks.push(make_hunk(
                &path,
                h.old_start,
                h.new_start,
                &h.header,
                h.lines.clone(),
                &mut used_ids,
            ));
        }
        // 二进制、权限、纯重命名、空文件这类没有文本差异的变化，用一行说明代替，也要讲到
        for note in file_notes(f) {
            hunks.push(make_hunk(
                &path,
                0,
                0,
                "",
                vec![format!("\\ {note}")],
                &mut used_ids,
            ));
        }
    }
    (files, hunks)
}

fn file_notes(f: &FileDiff) -> Vec<String> {
    let mut notes = Vec::new();
    if !f.note.is_empty() {
        notes.push(f.note.clone());
    }
    if f.binary {
        notes.push("二进制文件有变化，无法显示内容差异".to_string());
    }
    if !f.old_mode.is_empty() && !f.new_mode.is_empty() && f.old_mode != f.new_mode {
        notes.push(format!("文件权限变化：{} → {}", f.old_mode, f.new_mode));
    }
    if f.hunks.is_empty() && notes.is_empty() {
        notes.push(match f.status {
            "renamed" => format!(
                "从 {} 重命名而来，内容没有变化",
                f.old_path.as_deref().unwrap_or("")
            ),
            "added" => "新建了一个空文件".to_string(),
            "deleted" => "删除了一个空文件".to_string(),
            _ => "文件有变化，但没有可显示的文本差异".to_string(),
        });
    }
    notes
}

fn make_hunk(
    file: &str,
    old_start: u64,
    new_start: u64,
    header: &str,
    lines: Vec<String>,
    used_ids: &mut HashSet<String>,
) -> Hunk {
    let mut sha = sha1_smol::Sha1::new();
    sha.update(file.as_bytes());
    sha.update(b"\0");
    sha.update(lines.join("\n").as_bytes());
    let digest = sha.digest().to_string();
    let short = &digest[..6];
    let mut id = format!("h{short}");
    // 同一个文件里出现完全相同的改动时，按出现顺序加后缀区分
    let mut n = 2;
    while used_ids.contains(&id) {
        id = format!("h{short}-{n}");
        n += 1;
    }
    used_ids.insert(id.clone());
    Hunk {
        id,
        file: file.to_string(),
        old_start,
        new_start,
        lines,
        header: header.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn hunk_lines_follow_header_counts() {
        // 内容里的 "--- " 不会被当成文件头
        let files = parse_unified_diff(
            &[
                "diff --git a/db.sql b/db.sql",
                "index 1111111..2222222 100644",
                "--- a/db.sql",
                "+++ b/db.sql",
                "@@ -1,3 +1,3 @@ CREATE TABLE",
                " select 1;",
                "--- 旧注释",
                "+-- 新注释",
                " select 2;",
                "",
            ]
            .join("\n"),
        );
        let file = &files[0];
        assert_eq!(file.new_path.as_deref(), Some("db.sql"));
        assert_eq!(file.status, "modified");
        assert_eq!(file.hunks.len(), 1);
        assert_eq!(file.hunks[0].header, "CREATE TABLE");
        assert_eq!(
            file.hunks[0].lines,
            lines(&[" select 1;", "--- 旧注释", "+-- 新注释", " select 2;"])
        );
    }

    #[test]
    fn added_deleted_renamed_binary_and_mode_changes() {
        let files = parse_unified_diff(
            &[
                "diff --git a/src/new.ts b/src/new.ts",
                "new file mode 100644",
                "index 0000000..3333333",
                "--- /dev/null",
                "+++ b/src/new.ts",
                "@@ -0,0 +1,2 @@",
                "+export const a = 1;",
                "+export const b = 2;",
                "\\ No newline at end of file",
                "diff --git a/old.txt b/old.txt",
                "deleted file mode 100644",
                "index 4444444..0000000",
                "--- a/old.txt",
                "+++ /dev/null",
                "@@ -1 +0,0 @@",
                "-bye",
                "diff --git a/src/config.ts b/src/settings.ts",
                "similarity index 80%",
                "rename from src/config.ts",
                "rename to src/settings.ts",
                "index 5555555..6666666 100644",
                "--- a/src/config.ts",
                "+++ b/src/settings.ts",
                "@@ -1,2 +1,2 @@",
                " export const API = '/api';",
                "-export const SIZE = 20;",
                "+export const SIZE = 50;",
                "diff --git a/a.md b/docs/a.md",
                "similarity index 100%",
                "rename from a.md",
                "rename to docs/a.md",
                "diff --git a/logo.png b/logo.png",
                "index 7777777..8888888 100644",
                "Binary files a/logo.png and b/logo.png differ",
                "diff --git a/run.sh b/run.sh",
                "old mode 100644",
                "new mode 100755",
                "",
            ]
            .join("\n"),
        );
        let [added, deleted, renamed, moved, binary, mode] = files.as_slice() else {
            panic!("应该解析出 6 个文件，实际 {}", files.len());
        };

        assert_eq!(added.status, "added");
        assert_eq!(added.new_path.as_deref(), Some("src/new.ts"));
        assert_eq!(
            added.hunks[0].lines,
            lines(&[
                "+export const a = 1;",
                "+export const b = 2;",
                "\\ No newline at end of file"
            ])
        );

        assert_eq!(deleted.status, "deleted");
        assert_eq!(deleted.old_path.as_deref(), Some("old.txt"));
        assert_eq!(
            (deleted.hunks[0].old_count, deleted.hunks[0].new_count),
            (1, 0)
        );
        assert_eq!(deleted.hunks[0].lines, lines(&["-bye"]));

        assert_eq!(renamed.status, "renamed");
        assert_eq!(renamed.old_path.as_deref(), Some("src/config.ts"));
        assert_eq!(renamed.new_path.as_deref(), Some("src/settings.ts"));
        assert_eq!(renamed.hunks.len(), 1);

        assert_eq!(moved.status, "renamed");
        assert_eq!(moved.new_path.as_deref(), Some("docs/a.md"));
        assert!(moved.hunks.is_empty());

        assert!(binary.binary);
        assert_eq!(binary.new_path.as_deref(), Some("logo.png"));

        assert_eq!(
            (mode.old_mode.as_str(), mode.new_mode.as_str()),
            ("100644", "100755")
        );
    }

    #[test]
    fn spaced_and_quoted_paths_with_crlf() {
        let files = parse_unified_diff(
            &[
                "diff --git a/my file.txt b/my file.txt",
                "index 1111111..2222222 100644",
                "--- a/my file.txt\t",
                "+++ b/my file.txt\t",
                "@@ -1 +1 @@\r",
                "-hello\r",
                "+hello world\r",
                "diff --git \"a/\\346\\226\\207\\344\\273\\266.txt\" \"b/\\346\\226\\207\\344\\273\\266.txt\"",
                "index 3333333..4444444 100644",
                "--- \"a/\\346\\226\\207\\344\\273\\266.txt\"",
                "+++ \"b/\\346\\226\\207\\344\\273\\266.txt\"",
                "@@ -1 +1 @@",
                "-x",
                "+y",
                "",
            ]
            .join("\n"),
        );
        assert_eq!(files[0].new_path.as_deref(), Some("my file.txt"));
        assert_eq!(files[0].hunks[0].lines, lines(&["-hello", "+hello world"]));
        assert_eq!(files[1].old_path.as_deref(), Some("文件.txt"));
        assert_eq!(files[1].new_path.as_deref(), Some("文件.txt"));
    }

    #[test]
    fn unquote_c_style_escapes() {
        assert_eq!(unquote("plain.txt"), "plain.txt");
        assert_eq!(unquote("\"a\\tb\\\"c\\\\d\""), "a\tb\"c\\d");
        assert_eq!(unquote("\"\\346\\226\\207\""), "文");
    }

    #[test]
    fn untracked_text_splits_like_javascript() {
        assert_eq!(split_lines("a\r\nb\n"), vec!["a", "b", ""]);
        assert_eq!(split_lines("a\rb\r"), vec!["a\rb\r"]);
    }

    #[test]
    fn same_change_in_one_file_gets_suffixed_ids() {
        let mut used = HashSet::new();
        let a = make_hunk("x.ts", 1, 1, "", lines(&["+a"]), &mut used);
        let b = make_hunk("x.ts", 9, 9, "", lines(&["+a"]), &mut used);
        assert_eq!(b.id, format!("{}-2", a.id));
        assert!(a.id.starts_with('h') && a.id.len() == 7);
    }
}
