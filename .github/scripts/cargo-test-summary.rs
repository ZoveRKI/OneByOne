//! 将 cargo test 的输出写入 GitHub Actions Job Summary，格式参照 Vitest 的测试报告，仅使用标准库。
//!
//! 用法：cargo-test-summary --outcome success|failure <cargo test 的输出文件>
//!
//! libtest 的 JSON 输出要 nightly 才能用，所以这里解析 `cargo test 2>&1` 的文本：cargo 运行每个测试目标前
//! 打印 `Running ...` 或 `Doc-tests ...`，测试程序逐行打印 `test <名称> ... ok`，最后打印 `test result:`。
//! 需要默认的输出格式，cargo test 不要加 `-q`。

use std::{
    env,
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::Write as _,
    process::ExitCode,
};

const MAX_FAILURES: usize = 50;

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Fail,
    Pass,
    Skip,
    Incomplete,
}

#[derive(Clone, Copy, Default)]
struct Counts {
    fail: usize,
    pass: usize,
    skip: usize,
    incomplete: usize,
}

impl Counts {
    fn add(&mut self, status: Status) {
        match status {
            Status::Fail => self.fail += 1,
            Status::Pass => self.pass += 1,
            Status::Skip => self.skip += 1,
            Status::Incomplete => self.incomplete += 1,
        }
    }

    fn merge(&mut self, other: Counts) {
        self.fail += other.fail;
        self.pass += other.pass;
        self.skip += other.skip;
        self.incomplete += other.incomplete;
    }

    fn total(&self) -> usize {
        self.fail + self.pass + self.skip + self.incomplete
    }
}

/// 一个测试目标：src/ 里的单元测试、tests/ 里的一个集成测试文件，或者 doc test
struct Target {
    name: String,
    /// `running N tests` 里的 N
    expected: Option<usize>,
    /// 逐行读到的测试结果
    seen: Counts,
    failed_tests: Vec<String>,
    /// `test result:` 行：是否通过，以及 libtest 统计的数量
    result: Option<(bool, Counts)>,
    /// cargo 报告这个目标失败。测试程序崩溃时只有这一条，没有 `test result:`
    failed: bool,
}

impl Target {
    fn new(name: String) -> Self {
        Target {
            name,
            expected: None,
            seen: Counts::default(),
            failed_tests: Vec::new(),
            result: None,
            failed: false,
        }
    }

    fn read_line(&mut self, line: &str) {
        if let Some(rest) = line.strip_prefix("running ") {
            let count = rest
                .strip_suffix(" tests")
                .or_else(|| rest.strip_suffix(" test"));
            if let Some(count) = count.and_then(|count| count.parse().ok()) {
                self.expected.get_or_insert(count);
            }
        } else if let Some(rest) = line.strip_prefix("test result: ") {
            self.result = Some(parse_result(rest));
        } else if let Some((name, outcome)) = line
            .strip_prefix("test ")
            .and_then(|rest| rest.rsplit_once(" ... "))
        {
            match outcome {
                "ok" => self.seen.add(Status::Pass),
                "FAILED" => {
                    self.seen.add(Status::Fail);
                    // doc test 的名称里有文件路径，Windows 上是反斜杠
                    self.failed_tests.push(name.replace('\\', "/"));
                }
                // 带原因时是 `ignored, <原因>`
                _ if outcome.starts_with("ignored") => self.seen.add(Status::Skip),
                _ => {}
            }
        } else if line.starts_with("error: test failed")
            || line.starts_with("error: doctest failed")
        {
            self.failed = true;
        }
    }

    fn status(&self) -> Status {
        if self.failed {
            return Status::Fail;
        }
        match self.result {
            None => Status::Incomplete,
            Some((false, _)) => Status::Fail,
            // 一个测试都没运行（没有测试，或者全部被忽略），和 Go 里没有测试文件的包一样记为 skip
            Some((true, counts)) if counts.pass == 0 => Status::Skip,
            Some(_) => Status::Pass,
        }
    }

    fn tests(&self) -> Counts {
        if let Some((_, counts)) = self.result {
            return counts;
        }
        // 测试程序中途退出，没有 `test result:`：用逐行读到的结果，没跑完的记为 incomplete
        let mut counts = self.seen;
        counts.incomplete = self.expected.unwrap_or(0).saturating_sub(counts.total());
        counts
    }
}

/// 解析 `test result:` 之后的部分，例如 `ok. 17 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.01s`
fn parse_result(rest: &str) -> (bool, Counts) {
    let (state, stats) = rest.split_once(". ").unwrap_or((rest, ""));
    let mut counts = Counts::default();
    for stat in stats.split("; ") {
        let mut words = stat.split(' ');
        let (Some(number), Some(kind)) = (words.next(), words.next()) else {
            continue;
        };
        let Ok(number) = number.parse() else {
            continue;
        };
        match kind {
            "passed" => counts.pass = number,
            "failed" => counts.fail = number,
            "ignored" => counts.skip = number,
            _ => {}
        }
    }
    (state == "ok", counts)
}

#[derive(Default)]
struct Results {
    targets: Vec<Target>,
    /// `error: could not compile` 报告的编译失败，例如 onebyone (test "cli")
    build_failures: Vec<String>,
}

fn parse(output: &str) -> Results {
    let mut results = Results::default();
    for line in output.lines() {
        let line = strip_ansi(line);
        // cargo 的状态词右对齐到第 12 列：`     Running unittests src/lib.rs (target/debug/deps/onebyone-…)`
        if let Some(rest) = line.strip_prefix("     Running ") {
            let name = rest.rsplit_once(" (").map_or(rest, |(name, _)| name);
            results.targets.push(Target::new(name.replace('\\', "/")));
        } else if let Some(name) = line.strip_prefix("   Doc-tests ") {
            results
                .targets
                .push(Target::new(format!("Doc-tests {name}")));
        } else if let Some(rest) = line.strip_prefix("error: could not compile ") {
            let name = rest.split_once(" due to ").map_or(rest, |(name, _)| name);
            results.build_failures.push(name.replace('`', ""));
        } else if let Some(target) = results.targets.last_mut() {
            target.read_line(&line);
        }
    }
    results
}

/// 去掉颜色等 ANSI 控制序列，设置了 CARGO_TERM_COLOR=always 时 cargo 会输出这些序列
fn strip_ansi(line: &str) -> String {
    let mut plain = String::with_capacity(line.len());
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' && characters.next_if_eq(&'[').is_some() {
            // ESC [ 之后一直到 @ 到 ~ 之间的结束字符
            for character in characters.by_ref() {
                if ('@'..='~').contains(&character) {
                    break;
                }
            }
        } else {
            plain.push(character);
        }
    }
    plain
}

// 一次性转义原始字符，避免 Markdown 转义再次改写 HTML 实体中的 #。
fn code(value: &str) -> String {
    let mut safe = String::new();
    for (index, character) in value.chars().enumerate() {
        if index == 500 {
            safe.push('…');
            break;
        }
        match character {
            '\n' | '\r' => safe.push(' '),
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '|' | '~' => {
                let _ = write!(safe, "&#{};", u32::from(character));
            }
            '&' => safe.push_str("&amp;"),
            '\'' => safe.push_str("&#39;"),
            '<' => safe.push_str("&lt;"),
            '>' => safe.push_str("&gt;"),
            '"' => safe.push_str("&#34;"),
            _ => safe.push(character),
        }
    }
    format!("<code>{safe}</code>")
}

/// 和 Vitest 的统计行写法相同，例如 `- **Test Results**: ❌ **1 failure** · ✅ **27 passes** · 28 total`
fn summary_line(label: &str, counts: Counts) -> String {
    let mut parts = Vec::new();
    for (count, singular, plural, icon) in [
        (counts.fail, "failure", "failures", "❌"),
        (counts.pass, "pass", "passes", "✅"),
        (counts.skip, "skip", "skips", ""),
        (counts.incomplete, "incomplete", "incomplete", "⚠️"),
    ] {
        if count == 0 {
            continue;
        }
        let part = format!("{count} {}", if count == 1 { singular } else { plural });
        parts.push(if icon.is_empty() {
            part
        } else {
            format!("{icon} **{part}**")
        });
    }
    parts.push(format!("{} total", counts.total()));
    format!("- **{label}**: {}", parts.join(" · "))
}

fn render_report(path: &str, outcome: &str) -> (String, bool) {
    let mut problems = Vec::new();
    let results = match fs::read(path) {
        Ok(bytes) => parse(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}')),
        Err(_) => {
            problems.push("Could not read test results. See the Backend Tests step logs.");
            Results::default()
        }
    };
    if results.targets.is_empty() {
        problems
            .push("No test target results were received. Test statistics could not be confirmed.");
    }
    let mut target_counts = Counts::default();
    let mut test_counts = Counts::default();
    let mut failed_tests = Vec::new();
    let mut failed_targets = Vec::new();
    for target in &results.targets {
        let status = target.status();
        target_counts.add(status);
        test_counts.merge(target.tests());
        failed_tests.extend(
            target
                .failed_tests
                .iter()
                .map(|name| (target.name.as_str(), name.as_str())),
        );
        if status == Status::Fail && target.failed_tests.is_empty() {
            failed_targets.push(target.name.as_str());
        }
    }
    if target_counts.incomplete + test_counts.incomplete > 0 {
        problems.push(
            "Some test targets or tests have no completion record. Statistics may be incomplete.",
        );
    }
    failed_tests.sort();
    failed_targets.sort();
    let mut build_failures: Vec<&str> = results.build_failures.iter().map(String::as_str).collect();
    build_failures.sort();
    let failures: Vec<(&str, &str)> = failed_tests
        .into_iter()
        .chain(
            failed_targets
                .into_iter()
                .map(|target| (target, "Target failure; see test logs")),
        )
        .chain(
            build_failures
                .into_iter()
                .map(|build| (build, "Build failure; see test logs")),
        )
        .collect();
    let failed = outcome == "failure" || !failures.is_empty();

    let mut lines = vec![
        "## Rust Test Report".to_string(),
        String::new(),
        "### Summary".to_string(),
        String::new(),
    ];
    if failed {
        lines.extend(["**Result: ❌ Failed**".to_string(), String::new()]);
    } else if !problems.is_empty() {
        lines.extend(["**Result: ⚠️ Incomplete**".to_string(), String::new()]);
    }
    lines.push(summary_line("Test Targets", target_counts));
    lines.push(summary_line("Test Results", test_counts));
    if !problems.is_empty() {
        lines.push(String::new());
        lines.extend(problems.iter().map(|problem| format!("> ⚠️ {problem}")));
    }
    if !failures.is_empty() {
        lines.extend([String::new(), "### Failures".to_string(), String::new()]);
        for (target, name) in failures.iter().take(MAX_FAILURES) {
            lines.push(format!("- {}: {}", code(target), code(name)));
        }
        if failures.len() > MAX_FAILURES {
            lines.extend([
                String::new(),
                format!(
                    "Showing the first {MAX_FAILURES} of {} failures.",
                    failures.len()
                ),
            ]);
        }
    }
    if failed {
        lines.extend([
            String::new(),
            "See **Backend Tests** step logs for full error output.".to_string(),
        ]);
    }
    (lines.join("\n") + "\n", !problems.is_empty())
}

/// 读取 `--outcome success|failure <文件>`，也接受 `--outcome=success` 的写法
fn parse_args(mut args: impl Iterator<Item = String>) -> Option<(String, String)> {
    let mut outcome = None;
    let mut files = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--outcome" {
            outcome = args.next();
        } else if let Some(value) = arg.strip_prefix("--outcome=") {
            outcome = Some(value.to_string());
        } else {
            files.push(arg);
        }
    }
    let outcome = outcome.filter(|outcome| outcome == "success" || outcome == "failure")?;
    let [file] = <[String; 1]>::try_from(files).ok()?;
    Some((outcome, file))
}

fn main() -> ExitCode {
    let Some((outcome, path)) = parse_args(env::args().skip(1)) else {
        eprintln!("Usage: cargo-test-summary --outcome success|failure <cargo-test-output.txt>");
        return ExitCode::from(2);
    };
    let (report, incomplete) = render_report(&path, &outcome);
    let summary_path = env::var_os("GITHUB_STEP_SUMMARY").unwrap_or_default();
    let mut summary = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(summary_path)
    {
        Ok(file) => file,
        Err(error) => {
            eprintln!("Could not open GitHub Actions Job Summary: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = summary.write_all(report.as_bytes()) {
        eprintln!("Could not write GitHub Actions Job Summary: {error}");
        return ExitCode::FAILURE;
    }
    if incomplete {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
