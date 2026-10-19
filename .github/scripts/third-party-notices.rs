//! 生成发布包里的第三方许可证声明（THIRD-PARTY-NOTICES.md）。
//!
//! 用法：cargo run --manifest-path .github/scripts/Cargo.toml --bin third-party-notices -- <Rust 目标平台> <输出文件>
//! 例如：cargo run --manifest-path .github/scripts/Cargo.toml --bin third-party-notices -- x86_64-pc-windows-msvc dist/THIRD-PARTY-NOTICES.md
//!
//! 发布的程序里有两类第三方代码：编进 onebyone 的 Rust 依赖，和打包进内嵌播放器页面的前端依赖。
//! 它们的许可证要求分发时附上版权和许可声明，这里收集每个依赖自带的 LICENSE、NOTICE 等文件，
//! 相同的全文只写一次。依赖信息来自 cargo metadata 和 pnpm licenses list，不需要额外安装工具。

use serde_json::Value;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    env, fs,
    path::Path,
    process::{Command, ExitCode, Stdio},
};

struct LicenseFile {
    name: String,
    text: String,
}

struct Package {
    name: String,
    version: String,
    license: String,
    url: String,
    files: Vec<LicenseFile>,
}

/// LICENSE、LICENSE-MIT、COPYING、NOTICE、UNLICENSE 等，不区分大小写
fn is_license_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["license", "licence", "copying", "notice", "unlicense"]
        .iter()
        .any(|stem| {
            name.strip_prefix(stem)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '.', '_']))
        })
}

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    // 和 JS 的 trim() 一样，开头的 BOM 也去掉
    Ok(text
        .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
        .to_string())
}

fn license_files(dir: &Path, extra: Option<&str>) -> Result<Vec<LicenseFile>, String> {
    // 目录不存在时当作没有许可证文件
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().is_file())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .filter(|name| is_license_file(name))
                .collect()
        })
        .unwrap_or_default();
    if let Some(extra) = extra
        && !names.iter().any(|name| name == extra)
    {
        names.push(extra.to_string());
    }
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let text = read_text(&dir.join(&name))?;
            Ok(LicenseFile { name, text })
        })
        .collect()
}

/// 运行命令并返回标准输出；标准错误照常显示，方便看到出错原因
fn run(command: &mut Command) -> Result<String, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("无法运行 {program}：{error}"))?;
    if !output.status.success() {
        return Err(format!("{program} 运行失败（{}）", output.status));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{program} 的输出不是 UTF-8"))
}

fn parse_json(text: &str, source: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("{source} 的输出不是有效的 JSON：{error}"))
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn text_of(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn rust_packages(root: &Path, target: &str) -> Result<Vec<Package>, String> {
    let json = run(Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--locked"])
        .args(["--filter-platform", target])
        .current_dir(root.join("server")))?;
    let meta = parse_json(&json, "cargo metadata")?;
    let packages: HashMap<&str, &Value> = items(&meta["packages"])
        .map(|p| (text_of(&p["id"]), p))
        .collect();
    let nodes: HashMap<&str, &Value> = items(&meta["resolve"]["nodes"])
        .map(|n| (text_of(&n["id"]), n))
        .collect();
    let root_id = text_of(&meta["resolve"]["root"]);
    // 从 onebyone 出发只沿普通依赖走：开发依赖（测试用）和构建依赖（build.rs 用）不会进入发布的程序
    let mut used = HashSet::new();
    let mut pending = vec![root_id];
    while let Some(id) = pending.pop() {
        if !used.insert(id) {
            continue;
        }
        let node = nodes
            .get(id)
            .ok_or_else(|| format!("cargo metadata 里缺少 {id} 的依赖关系"))?;
        for dep in items(&node["deps"]) {
            if items(&dep["dep_kinds"]).any(|k| k["kind"].is_null()) {
                pending.push(text_of(&dep["pkg"]));
            }
        }
    }
    used.remove(root_id);
    used.into_iter()
        .map(|id| {
            let p = packages
                .get(id)
                .ok_or_else(|| format!("cargo metadata 里缺少 {id} 的信息"))?;
            let name = text_of(&p["name"]).to_string();
            let dir = Path::new(text_of(&p["manifest_path"]))
                .parent()
                .unwrap_or(Path::new(""));
            Ok(Package {
                version: text_of(&p["version"]).to_string(),
                license: p["license"].as_str().unwrap_or("见许可证文件").to_string(),
                url: p["repository"]
                    .as_str()
                    .or(p["homepage"].as_str())
                    .map_or_else(|| format!("https://crates.io/crates/{name}"), String::from),
                files: license_files(dir, p["license_file"].as_str())?,
                name,
            })
        })
        .collect()
}

fn web_packages(root: &Path) -> Result<Vec<Package>, String> {
    // Windows 上的 pnpm 可能是 .cmd 脚本，Command 按名字找不到，要经过 cmd 启动；命令是固定的，不涉及外部输入
    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "pnpm"]);
        command
    } else {
        Command::new("pnpm")
    };
    command
        .args(["licenses", "list", "--prod", "--json"])
        .current_dir(root.join("web"));
    let licenses = parse_json(&run(&mut command)?, "pnpm licenses list")?;
    let mut packages = Vec::new();
    // 按许可证分组：{ "MIT": [{ name, versions, paths, license, homepage }, ...], ... }
    for p in licenses
        .as_object()
        .into_iter()
        .flat_map(|groups| groups.values())
        .flat_map(items)
    {
        let name = text_of(&p["name"]);
        let paths: Vec<&str> = items(&p["paths"]).map(text_of).collect();
        for (i, version) in items(&p["versions"]).enumerate() {
            packages.push(Package {
                name: name.to_string(),
                version: text_of(version).to_string(),
                license: text_of(&p["license"]).to_string(),
                url: p["homepage"].as_str().map_or_else(
                    || format!("https://www.npmjs.com/package/{name}"),
                    String::from,
                ),
                files: license_files(Path::new(paths.get(i).copied().unwrap_or_default()), None)?,
            });
        }
    }
    Ok(packages)
}

/// 按名称排序（不区分大小写），名称相同时按版本
fn by_name(a: &Package, b: &Package) -> Ordering {
    a.name
        .to_lowercase()
        .cmp(&b.name.to_lowercase())
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.version.cmp(&b.version))
}

/// 代码块的围栏要比全文里最长的一串反引号更长，避免提前结束
fn fence(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat((longest + 1).max(3))
}

fn cell(value: &str) -> String {
    value.replace('|', "\\|")
}

/// 内容相同的许可证全文只写一次，记下文件名和使用它的依赖
struct LicenseText<'a> {
    text: &'a str,
    names: Vec<&'a str>,
    users: Vec<String>,
}

fn generate(target: &str, output: &Path) -> Result<String, String> {
    // 仓库根目录：本文件在 .github/scripts/ 下
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .ok_or("找不到仓库根目录")?;
    let mut rust = rust_packages(root, target)?;
    let mut web = web_packages(root)?;
    rust.sort_by(by_name);
    web.sort_by(by_name);
    let groups = [
        ("Rust 依赖（编进 onebyone 程序）", rust),
        ("前端依赖（打包进内嵌的播放器页面）", web),
    ];

    let mut texts: Vec<LicenseText> = Vec::new();
    let mut positions: HashMap<&str, usize> = HashMap::new();
    for (_, packages) in &groups {
        for p in packages {
            for file in &p.files {
                let position = *positions.entry(&file.text).or_insert_with(|| {
                    texts.push(LicenseText {
                        text: &file.text,
                        names: Vec::new(),
                        users: Vec::new(),
                    });
                    texts.len() - 1
                });
                let entry = &mut texts[position];
                entry.users.push(format!("{} {}", p.name, p.version));
                if !entry.names.contains(&file.name.as_str()) {
                    entry.names.push(&file.name);
                }
            }
        }
    }

    let mut out = vec![
        "# 第三方许可证声明".to_string(),
        String::new(),
        "OneByOne 的发布包里包含以下第三方软件，它们按各自的许可证授权。下面先列出每个依赖的名称、版本和许可证，再附上依赖自带的许可证文件全文（内容相同的只列一次）。".to_string(),
        String::new(),
        format!("本文件由 `.github/scripts/third-party-notices.rs` 生成，对应的平台：`{target}`。"),
        String::new(),
    ];
    for (title, packages) in &groups {
        out.extend([
            format!("## {title}"),
            String::new(),
            "| 名称 | 版本 | 许可证 | 来源 |".to_string(),
            "|---|---|---|---|".to_string(),
        ]);
        for p in packages {
            out.push(format!(
                "| {} | {} | {} | {} |",
                cell(&p.name),
                p.version,
                cell(&p.license),
                p.url
            ));
        }
        out.push(String::new());
    }
    let missing: Vec<&Package> = groups
        .iter()
        .flat_map(|(_, packages)| packages)
        .filter(|p| p.files.is_empty())
        .collect();
    if !missing.is_empty() {
        out.push("以下依赖的发布包里没有附带许可证文件，许可证以上表为准；MIT、Apache-2.0 等标准许可证的全文，见下方其他依赖附带的同类文件，也可以到它们的来源查看：".to_string());
        out.push(String::new());
        out.extend(
            missing
                .iter()
                .map(|p| format!("- {} {}（{}）", p.name, p.version, p.license)),
        );
        out.push(String::new());
    }
    out.extend(["## 许可证全文".to_string(), String::new()]);
    for (n, entry) in texts.iter().enumerate() {
        let f = fence(entry.text);
        out.extend([
            format!("### {}. {}", n + 1, entry.names.join("、")),
            String::new(),
            format!("使用它的依赖：{}", entry.users.join("、")),
            String::new(),
            f.clone(),
            entry.text.to_string(),
            f,
            String::new(),
        ]);
    }

    if let Some(dir) = output.parent() {
        fs::create_dir_all(dir).map_err(|error| format!("创建 {} 失败：{error}", dir.display()))?;
    }
    fs::write(output, out.join("\n"))
        .map_err(|error| format!("写入 {} 失败：{error}", output.display()))?;
    let total: usize = groups.iter().map(|(_, packages)| packages.len()).sum();
    Ok(format!(
        "已生成 {}：{total} 个依赖，{} 份许可证全文",
        output.display(),
        texts.len()
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [target, output] = args.as_slice() else {
        eprintln!(
            "用法：cargo run --manifest-path .github/scripts/Cargo.toml --bin third-party-notices -- <Rust 目标平台> <输出文件>"
        );
        return ExitCode::FAILURE;
    };
    match generate(target, Path::new(output)) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
