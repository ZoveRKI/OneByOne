//! 生成给 AI 读的改动清单 `.onebyone/hunks.md`：按文件列出每个改动块和编号，文末附上写法要求。

use crate::diff::{Diff, Hunk};

// 清单里每个改动块最多列出的行数；更长的只列开头，完整内容在播放器里看
const LISTING_MAX_LINES: usize = 120;
const GUIDE: &str = include_str!("../assets/guide.md");

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

pub fn render(diff: &Diff, range: &str, cli: &str) -> String {
    let (adds, dels) = count_lines(&diff.hunks);
    let mut out = vec![
        "# OneByOne 改动清单".to_string(),
        String::new(),
        format!("- 对比范围：{range}"),
        format!(
            "- {} 个文件，{} 个改动块，+{adds} −{dels}",
            diff.files.len(),
            diff.hunks.len()
        ),
        String::new(),
        "请按文末的「写法要求」，把下面的改动块编排成讲解步骤，写入 `.onebyone/steps.json`。"
            .to_string(),
        String::new(),
    ];
    for f in &diff.files {
        let list: Vec<&Hunk> = diff.hunks.iter().filter(|h| h.file == f.path).collect();
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
    out.push(GUIDE.replace("{{ONEBYONE}}", cli));
    out.join("\n")
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
}
