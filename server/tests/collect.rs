mod common;

use common::TempRepo;
use onebyone::diff::{CollectOptions, Diff, collect_diff};
use std::{fs, path::Path};

fn collect(cwd: &Path, paths: &[String]) -> Diff {
    collect_diff(&CollectOptions {
        cwd,
        base: "HEAD",
        head: None,
        paths,
    })
    .expect("收集改动失败")
}

fn ids(diff: &Diff) -> Vec<String> {
    diff.hunks.iter().map(|h| h.id.clone()).collect()
}

#[test]
fn modified_deleted_renamed_untracked_and_binary_files() {
    let repo = TempRepo::new();
    repo.write("src/a.ts", "const a = 1;\nconst b = 2;\n");
    repo.write("src/old.ts", "export {};\n");
    repo.write("README.md", "# demo\n\nhello\n");
    repo.write("logo.bin", [0u8, 1, 2, 3]);
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "init"]);

    repo.write("src/a.ts", "const a = 1;\nconst b = 3;\n");
    fs::remove_file(repo.path("src/old.ts")).unwrap();
    repo.write("docs/.keep", "");
    repo.git(&["mv", "README.md", "docs/README.md"]);
    repo.write("src/new.ts", "export const c = 1;");
    repo.write("logo.bin", [0u8, 1, 2, 4]);
    repo.write(".onebyone/hunks.md", "生成目录里的文件不应该被收集");

    let diff = collect(&repo.dir, &[]);
    let file = |p: &str| {
        diff.files
            .iter()
            .find(|f| f.path == p)
            .unwrap_or_else(|| panic!("缺少文件 {p}"))
    };
    assert_eq!(file("src/a.ts").status, "modified");
    assert_eq!(file("src/old.ts").status, "deleted");
    assert_eq!(file("docs/README.md").status, "renamed");
    assert_eq!(
        file("docs/README.md").old_path.as_deref(),
        Some("README.md")
    );
    assert_eq!(file("src/new.ts").status, "added");
    assert_eq!(file("docs/.keep").status, "added");
    assert_eq!(file("logo.bin").status, "modified");
    assert!(!diff.files.iter().any(|f| f.path.starts_with(".onebyone")));

    let lines_of = |p: &str| -> Vec<Vec<String>> {
        diff.hunks
            .iter()
            .filter(|h| h.file == p)
            .map(|h| h.lines.clone())
            .collect()
    };
    assert_eq!(
        lines_of("src/new.ts"),
        vec![vec!["+export const c = 1;", "\\ No newline at end of file"]]
    );
    assert!(lines_of("logo.bin")[0][0].contains("二进制"));
    assert!(lines_of("docs/README.md")[0][0].contains("重命名"));
    assert!(lines_of("docs/.keep")[0][0].contains("空文件"));

    // 内容不变时 id 不变；内容变了 id 跟着变
    assert_eq!(ids(&collect(&repo.dir, &[])), ids(&diff));
    repo.write("src/a.ts", "const a = 1;\nconst b = 4;\n");
    let id_of_a = |d: &Diff| {
        d.hunks
            .iter()
            .find(|h| h.file == "src/a.ts")
            .map(|h| h.id.clone())
    };
    assert_ne!(id_of_a(&collect(&repo.dir, &[])), id_of_a(&diff));

    // 只看部分路径
    let only_src = collect(&repo.dir, &["src".to_string()]);
    assert!(!only_src.files.is_empty());
    assert!(only_src.files.iter().all(|f| f.path.starts_with("src/")));
}

#[test]
fn repository_without_any_commit() {
    let repo = TempRepo::new();
    repo.write("hello.txt", "hi\n");
    let diff = collect(&repo.dir, &[]);
    assert_eq!(diff.base.commit, "");
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].status, "added");
    assert_eq!(diff.hunks[0].lines, vec!["+hi"]);
}

#[test]
fn paths_given_in_a_subdirectory_become_root_relative() {
    let repo = TempRepo::new();
    repo.write("web/src/a.ts", "a\n");
    repo.write("server/b.rs", "b\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "init"]);
    repo.write("web/src/a.ts", "a2\n");
    repo.write("server/b.rs", "b2\n");

    let diff = collect(
        &repo.path("web"),
        &["src".to_string(), "../server".to_string()],
    );
    assert_eq!(diff.paths, vec!["web/src", "server"]);
    let mut files: Vec<&str> = diff.files.iter().map(|f| f.path.as_str()).collect();
    files.sort_unstable();
    assert_eq!(files, vec!["server/b.rs", "web/src/a.ts"]);
}
