//! 生成给 AI 读的改动清单 `.onebyone/hunks.md`：按文件列出每个改动块和编号，文末附上写法要求。

use crate::diff::{Diff, Hunk};

// 清单里每个改动块最多列出的行数；更长的只列开头，完整内容在播放器里看
const LISTING_MAX_LINES: usize = 120;
// 写法要求分成两种用法各自的部分（步骤交到哪里、交完怎么检查）和共用的字段说明、编排要求
const GUIDE_CLI: &str = include_str!("../assets/guide.md");
const GUIDE_MCP: &str = include_str!("../assets/guide-mcp.md");
const GUIDE_STEPS: &str = include_str!("../assets/guide-steps.md");

/// 清单给哪种用法读，决定提示和写法要求的措辞
pub enum Target<'a> {
    /// 命令行：AI 写 steps.json，再用这个命令 build
    Cli(&'a str),
    /// MCP 工具：AI 把步骤作为参数调用 show_walkthrough
    Mcp,
}

/// 附在清单末尾的写法要求
pub fn guide(target: &Target) -> String {
    let guide = match target {
        Target::Cli(cli) => GUIDE_CLI.replace("{{ONEBYONE}}", cli),
        Target::Mcp => GUIDE_MCP.to_string(),
    };
    guide.replace("{{STEPS}}", GUIDE_STEPS.trim_end())
}

pub fn status_label(status: &str) -> &'static str {
    match status {
        "added" => "新增",
        "deleted" => "删除",
        "renamed" => "重命名",
        _ => "修改",
    }
}

/// 统计新增和删除的行数
pub fn count_lines<'a>(hunks: impl IntoIterator<Item = &'a Hunk>) -> (usize, usize) {
    let (mut adds, mut dels) = (0, 0);
    for h in hunks {
        for line in &h.lines {
            if line.starts_with('+') {
                adds += 1;
            } else if line.starts_with('-') {
                dels += 1;
            }
        }
    }
    (adds, dels)
}

/// 按改动行算出旧、新两侧的行数
pub fn raw_counts(lines: &[String]) -> (u64, u64) {
    let (mut old, mut new) = (0, 0);
    for line in lines {
        match line.chars().next() {
            Some('+') => new += 1,
            Some('-') => old += 1,
            Some('\\') => {}
            _ => {
                old += 1;
                new += 1;
            }
        }
    }
    (old, new)
}

/// 改动块的范围标题；二进制、纯重命名等没有文本差异的改动显示为「文件级变化」
pub fn hunk_label(old_start: u64, old_count: u64, new_start: u64, new_count: u64) -> String {
    if old_count == 0 && new_count == 0 {
        "文件级变化".to_string()
    } else {
        format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@")
    }
}

/// 完整的改动清单：每个改动块的编号和代码，文末附上写法要求
pub fn render(diff: &Diff, range: &str, target: &Target) -> String {
    let mut out = header(diff, range);
    out.push(match target {
        Target::Cli(_) => {
            "请按文末的「写法要求」，把下面的改动块编排成讲解步骤，写入 `.onebyone/steps.json`。"
        }
        Target::Mcp => {
            "请按文末的「写法要求」，把下面的改动块编排成讲解步骤，然后调用 `show_walkthrough`。"
        }
    }
    .to_string());
    out.push(String::new());
    for f in &diff.files {
        let list = hunks_of(diff, &f.path);
        let (a, d) = count_lines(list.iter().copied());
        out.push(format!(
            "## {}（{}，+{a} −{d}）",
            f.path,
            status_label(&f.status)
        ));
        out.push(String::new());
        if let Some(old) = &f.old_path {
            out.push(format!("由 `{old}` 重命名而来。"));
            out.push(String::new());
        }
        for h in list {
            let (old_count, new_count) = raw_counts(&h.lines);
            let header = if h.header.is_empty() {
                String::new()
            } else {
                format!(" {}", h.header)
            };
            out.push(format!(
                "### {} · {}{header}",
                h.id,
                hunk_label(h.old_start, old_count, h.new_start, new_count)
            ));
            out.push(String::new());
            let shown = &h.lines[..h.lines.len().min(LISTING_MAX_LINES)];
            let fence = fence_for(shown);
            out.push(format!("{fence}diff"));
            out.extend(shown.iter().cloned());
            out.push(fence);
            out.push(String::new());
            if shown.len() < h.lines.len() {
                out.push(format!(
                    "（这个改动块共 {} 行，这里只列出前 {LISTING_MAX_LINES} 行，播放器里会显示完整内容。）",
                    h.lines.len()
                ));
                out.push(String::new());
            }
        }
    }
    out.push(guide(target));
    out.join("\n")
}

/// 清单太长、不适合直接放进工具结果时的概要：只列文件，代码让 AI 到 `listing_path` 分段读
pub fn render_overview(diff: &Diff, range: &str, listing_path: &str) -> String {
    let mut out = header(diff, range);
    out.push(format!(
        "改动较多，完整的清单（每个改动块的编号和代码）已写入 `{listing_path}`。请分段读完这个文件，再按下面的「写法要求」编排步骤，然后调用 `show_walkthrough`。"
    ));
    out.push(String::new());
    out.push("涉及的文件：".to_string());
    out.push(String::new());
    for f in &diff.files {
        let list = hunks_of(diff, &f.path);
        let (a, d) = count_lines(list.iter().copied());
        out.push(format!(
            "- {}（{}，{} 个改动块，+{a} −{d}）",
            f.path,
            status_label(&f.status),
            list.len()
        ));
    }
    out.push(String::new());
    out.push(guide(&Target::Mcp));
    out.join("\n")
}

fn header(diff: &Diff, range: &str) -> Vec<String> {
    let (adds, dels) = count_lines(&diff.hunks);
    vec![
        "# OneByOne 改动清单".to_string(),
        String::new(),
        format!("- 对比范围：{range}"),
        format!(
            "- {} 个文件，{} 个改动块，+{adds} −{dels}",
            diff.files.len(),
            diff.hunks.len()
        ),
        String::new(),
    ]
}

fn hunks_of<'a>(diff: &'a Diff, path: &str) -> Vec<&'a Hunk> {
    diff.hunks.iter().filter(|h| h.file == path).collect()
}

// 代码里本身带有 ``` 时，外层用更长的反引号包起来，避免提前结束代码块
fn fence_for(lines: &[String]) -> String {
    let longest = lines
        .iter()
        .flat_map(|line| line.split(|c| c != '`').map(str::len))
        .max()
        .unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_is_longer_than_backticks_in_code() {
        assert_eq!(fence_for(&["a".to_string()]), "```");
        assert_eq!(
            fence_for(&["```js".to_string(), "x ``".to_string()]),
            "````"
        );
    }

    #[test]
    fn labels_and_counts() {
        let lines: Vec<String> = [" a", "-b", "+c", "\\ No newline at end of file"]
            .map(String::from)
            .to_vec();
        assert_eq!(raw_counts(&lines), (2, 2));
        assert_eq!(hunk_label(1, 2, 1, 2), "@@ -1,2 +1,2 @@");
        assert_eq!(hunk_label(0, 0, 0, 0), "文件级变化");
    }

    #[test]
    fn both_guides_share_the_step_rules() {
        for target in [Target::Cli("onebyone"), Target::Mcp] {
            let text = guide(&target);
            assert!(!text.contains("{{"), "占位符应该都被替换了：{text}");
            assert!(text.contains("| `hunks` |") && text.contains("1. **覆盖全部改动块**"));
        }
        assert!(guide(&Target::Cli("onebyone")).contains("onebyone build"));
        assert!(guide(&Target::Mcp).contains("调用 `show_walkthrough` 提交"));
    }
}
