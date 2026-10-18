//! 把前端打包好的播放器页面（web/dist/index.html）嵌进程序。
//! 还没构建前端时用一个提示页面代替，cargo build 和 cargo test 照样能运行。

use std::{env, fs, path::Path};

const PLACEHOLDER: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>OneByOne</title>
<script id="onebyone-data" type="application/json">null</script>
</head>
<body>
<p>播放器页面还没有构建。请在仓库根目录运行 <code>pnpm -C web build</code>，再重新编译命令行。</p>
</body>
</html>
"#;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("缺少 CARGO_MANIFEST_DIR");
    let out_dir = env::var("OUT_DIR").expect("缺少 OUT_DIR");
    let player = Path::new(&manifest_dir).join("../web/dist/index.html");
    let target = Path::new(&out_dir).join("player.html");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", player.display());

    let html = fs::read_to_string(&player).unwrap_or_else(|_| {
        println!(
            "cargo:warning=还没有构建播放器页面（web/dist/index.html），生成的页面只会显示提示。请先运行：pnpm -C web build"
        );
        PLACEHOLDER.to_string()
    });
    fs::write(&target, html).expect("写入 player.html 失败");
}
