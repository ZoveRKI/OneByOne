//! 生成播放页面：把讲解数据写进嵌入的播放器 HTML。生成的文件双击就能打开，不需要本地服务器。

use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Stdio},
};

// 前端打包好的单文件播放器，由 build.rs 在编译时嵌进来
const PLAYER: &str = include_str!(concat!(env!("OUT_DIR"), "/player.html"));
// 播放器启动时会读取这个标签里的 JSON；占位内容是 null
const DATA_TAG: &str = r#"<script id="onebyone-data" type="application/json">"#;

/// 把讲解数据写进播放器页面
pub fn render(data: &Value) -> Result<String, String> {
    render_with(PLAYER, data)
}

fn render_with(player: &str, data: &Value) -> Result<String, String> {
    let missing = || "播放器页面里找不到数据标签，请重新构建前端（pnpm -C web build）".to_string();
    let start = player.find(DATA_TAG).ok_or_else(missing)? + DATA_TAG.len();
    let end = start + player[start..].find("</script>").ok_or_else(missing)?;
    // JSON 里的 < 全部写成 \u003c：解析结果不变，又不会提前结束 script 标签
    let json = serde_json::to_string(data)
        .map_err(|e| format!("生成 JSON 失败：{e}"))?
        .replace('<', "\\u003c");
    Ok(format!("{}{json}{}", &player[..start], &player[end..]))
}

/// 用系统默认浏览器打开页面。设置了环境变量 ONEBYONE_NO_BROWSER 时只生成不打开（测试和自动化里用）
pub fn open_in_browser(path: &Path) -> Result<(), String> {
    if std::env::var_os("ONEBYONE_NO_BROWSER").is_some() {
        return Ok(());
    }
    let mut cmd = if cfg!(windows) {
        // 不经过 cmd.exe，路径里的 & 等字符不会被当成命令
        let mut c = Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else if cfg!(target_os = "macos") {
        Command::new("open")
    } else {
        Command::new("xdg-open")
    };
    cmd.arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开浏览器：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn data_replaces_placeholder_and_cannot_close_the_tag() {
        let player = "<head><script id=\"onebyone-data\" type=\"application/json\">\n  null\n</script></head>";
        let html = render_with(player, &json!({ "title": "</script><b>x" })).unwrap();
        assert_eq!(
            html,
            "<head><script id=\"onebyone-data\" type=\"application/json\">{\"title\":\"\\u003c/script>\\u003cb>x\"}</script></head>"
        );
    }

    #[test]
    fn embedded_player_has_the_data_tag() {
        assert!(PLAYER.contains(DATA_TAG));
    }
}
