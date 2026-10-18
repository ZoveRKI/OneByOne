mod common;

use common::TempRepo;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const CLI: &str = env!("CARGO_BIN_EXE_onebyone");

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(CLI)
        .args(args)
        .current_dir(dir)
        .env("ONEBYONE_NO_BROWSER", "1")
        .output()
        .expect("运行命令行失败")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("读取文件失败")).expect("不是合法的 JSON")
}

#[test]
fn hunks_then_steps_then_build() {
    let repo = TempRepo::new();
    repo.write("src/a.ts", "const a = 1;\n");
    repo.write("src/b.ts", "const b = 1;\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "init"]);
    repo.write("src/a.ts", "const a = 2;\n");
    repo.write("src/b.ts", "const b = 2;\n");
    repo.write("src/c.ts", "const c = 1;\n");

    let out = |name: &str| repo.path(".onebyone").join(name);
    let write_steps = |steps: Value| {
        let text = serde_json::to_string(&json!({ "title": "测试", "steps": steps })).unwrap();
        fs::write(out("steps.json"), text).unwrap();
    };

    let r = run(&repo.dir, &["hunks"]);
    assert_eq!(r.status.code(), Some(0), "{}", stderr(&r));
    let ids: Vec<String> = read_json(&out("hunks.json"))["hunks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 3);
    let listing = fs::read_to_string(out("hunks.md")).unwrap();
    for id in &ids {
        assert!(listing.contains(&format!("### {id}")), "清单里应该有 {id}");
    }
    assert!(listing.contains("## 写法要求") && listing.contains("1. **覆盖全部改动块**"));
    assert!(listing.contains("build"));
    assert!(!listing.contains("{{"), "占位符应该都被替换了");

    // 只讲了一个改动块，还引用了一个不存在的 id
    write_steps(json!([{ "title": "只讲一个", "hunks": [ids[0], "hdeadbe"] }]));
    let r = run(&repo.dir, &["build"]);
    assert_eq!(r.status.code(), Some(2), "{}", stderr(&r));
    assert!(
        stdout(&r).contains("还有 2 个改动块没有讲到"),
        "{}",
        stdout(&r)
    );
    assert!(stdout(&r).contains("不存在的改动块：hdeadbe"));

    // 再次运行 hunks 时，会说明已有步骤的情况
    let r = run(&repo.dir, &["hunks"]);
    assert!(
        stdout(&r).contains("已有 steps.json：1 处引用已失效，2 个改动块还没讲到"),
        "{}",
        stdout(&r)
    );

    // 全部讲到
    write_steps(json!([{ "title": "概览", "hunks": [] }, { "title": "全部改动", "hunks": ids }]));
    let r = run(&repo.dir, &["build"]);
    assert_eq!(r.status.code(), Some(0), "{}", stdout(&r));
    assert!(stdout(&r).contains("讲解覆盖了全部改动块"));
    let walkthrough = read_json(&out("walkthrough.json"));
    assert_eq!(walkthrough["hunks"].as_array().unwrap().len(), 3);
    assert_eq!(walkthrough["steps"].as_array().unwrap().len(), 2);
    assert_eq!(walkthrough["title"], "测试");
    // 播放页面里带着同一份数据
    let html = fs::read_to_string(out("walkthrough.html")).unwrap();
    assert!(html.contains(&format!("\"id\":\"{}\"", ids[0])));

    // 不带参数的 open 打开刚生成的页面
    let r = run(&repo.dir, &["open"]);
    assert_eq!(r.status.code(), Some(0), "{}", stderr(&r));
    assert!(stdout(&r).contains(".onebyone/walkthrough.html"));

    // 生成目录自带 .gitignore，不会出现在 git status 里
    assert!(!repo.git(&["status", "--porcelain"]).contains(".onebyone"));

    // hunks 之后改动又变了：旧引用失效，提示重新运行 hunks
    repo.write("src/a.ts", "const a = 3;\n");
    let r = run(&repo.dir, &["build"]);
    assert_eq!(r.status.code(), Some(2));
    assert!(
        stdout(&r).contains("运行 hunks 之后改动又有变化"),
        "{}",
        stdout(&r)
    );
}

#[test]
fn open_any_walkthrough_file() {
    let repo = TempRepo::empty();
    let data = json!({ "title": "示例 <b>", "hunks": [], "steps": [{ "title": "概览" }] });
    repo.write("demo.json", serde_json::to_string(&data).unwrap());
    let r = run(&repo.dir, &["open", "demo.json"]);
    assert_eq!(r.status.code(), Some(0), "{}", stderr(&r));
    let page = stdout(&r)
        .trim()
        .strip_prefix("播放页面：")
        .expect("应该输出页面路径")
        .to_string();
    let html = fs::read_to_string(&page).expect("页面应该已经生成");
    fs::remove_file(&page).ok();
    assert!(html.contains("示例 \\u003cb>"));

    repo.write("bad.json", "{");
    let r = run(&repo.dir, &["open", "bad.json"]);
    assert_eq!(r.status.code(), Some(1));
    assert!(
        stderr(&r).contains("bad.json 不是合法的 JSON"),
        "{}",
        stderr(&r)
    );
}

#[test]
fn install_skill_into_a_project() {
    let repo = TempRepo::empty();
    let r = run(&repo.dir, &["install-skill", "--project", "."]);
    assert_eq!(r.status.code(), Some(0), "{}", stderr(&r));
    let skill = fs::read_to_string(repo.path(".claude/skills/walkthrough/SKILL.md")).unwrap();
    assert!(!skill.contains("{{ONEBYONE}}"));
    assert!(skill.contains("onebyone"));
    assert!(stdout(&run(&repo.dir, &["install-skill", "--project", "."])).starts_with("已更新"));
}

#[test]
fn usage_errors_show_help() {
    let repo = TempRepo::empty();
    let r = run(&repo.dir, &["nope"]);
    assert_eq!(r.status.code(), Some(1));
    assert!(stderr(&r).contains("未知命令：nope") && stderr(&r).contains("用法："));
    let r = run(&repo.dir, &["hunks", "--base"]);
    assert!(stderr(&r).contains("--base 后面需要一个值"));
    let r = run(&repo.dir, &["build", "--base", "x"]);
    assert!(stderr(&r).contains("不支持的参数：--base"));
    let r = run(&repo.dir, &["--help"]);
    assert_eq!(r.status.code(), Some(0));
    assert!(stdout(&r).contains("onebyone hunks"));
}

#[test]
fn clear_error_outside_a_repository() {
    let dir = TempRepo::empty();
    let r = run(&dir.dir, &["hunks"]);
    // 系统临时目录恰好在某个 git 仓库里时，这个用例没有意义，直接跳过
    if r.status.code() == Some(0) {
        return;
    }
    assert_eq!(r.status.code(), Some(1));
    assert!(stderr(&r).contains("不在 git 仓库里"), "{}", stderr(&r));
}
