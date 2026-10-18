mod common;

use common::TempRepo;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

const CLI: &str = env!("CARGO_BIN_EXE_onebyone");
const PLAYER_URI: &str = "ui://onebyone/player.html";
const APP_MIME: &str = "text/html;profile=mcp-app";

/// 通过标准输入输出和 `onebyone mcp` 对话的测试客户端，每条消息占一行 JSON
struct Client {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
}

impl Client {
    /// 启动服务并完成初始化握手，返回客户端和 initialize 的结果
    fn start() -> (Self, Value) {
        let mut child = Command::new(CLI)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("启动 MCP 服务失败");
        let stdin = child.stdin.take().expect("拿不到标准输入");
        let stdout = child.stdout.take().expect("拿不到标准输出");
        // 在单独的线程里读输出，主线程等待响应时可以设超时，服务卡住时测试不会一直挂着
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            stdin,
            lines,
            next_id: 0,
        };
        let init = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "onebyone-test", "version": "0" },
                }),
            )
            .expect("初始化失败");
        client.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        (client, init)
    }

    fn send(&mut self, msg: &Value) {
        writeln!(self.stdin, "{msg}").expect("写入标准输入失败");
        self.stdin.flush().expect("写入标准输入失败");
    }

    /// 发出请求并等待对应的响应：成功时返回 result，协议错误时返回 error
    fn request(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(60))
                .expect("等待 MCP 响应超时");
            let msg: Value = serde_json::from_str(&line).expect("输出的不是合法的 JSON");
            if msg["id"] == id {
                return match msg.get("error") {
                    Some(error) => Err(error.clone()),
                    None => Ok(msg["result"].clone()),
                };
            }
        }
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        )
        .expect("调用工具不应该返回协议错误")
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text_of(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("读取文件失败")).expect("不是合法的 JSON")
}

#[test]
fn walkthrough_tool_points_to_the_player() {
    let (mut client, init) = Client::start();
    assert_eq!(init["serverInfo"]["name"], "onebyone");
    assert_eq!(
        init["capabilities"]["extensions"]["io.modelcontextprotocol/ui"]["mimeTypes"],
        json!([APP_MIME])
    );
    assert!(
        init["instructions"]
            .as_str()
            .unwrap()
            .contains("get_changes")
    );

    let listed = client.request("tools/list", json!({})).unwrap();
    let tools = listed["tools"].as_array().unwrap();
    let find = |name: &str| {
        tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("缺少工具 {name}"))
    };
    let show = find("show_walkthrough");
    assert_eq!(show["_meta"]["ui"]["resourceUri"], PLAYER_URI);
    assert_eq!(show["_meta"]["ui/resourceUri"], PLAYER_URI);
    assert_eq!(
        show["inputSchema"]["required"],
        json!(["repo", "title", "steps"])
    );
    // 只有 show_walkthrough 显示界面，get_changes 只返回文字
    assert!(find("get_changes").get("_meta").is_none());

    let resources = client.request("resources/list", json!({})).unwrap();
    assert_eq!(resources["resources"][0]["uri"], PLAYER_URI);
    assert_eq!(resources["resources"][0]["mimeType"], APP_MIME);
    let read = client
        .request("resources/read", json!({ "uri": PLAYER_URI }))
        .unwrap();
    let page = &read["contents"][0];
    assert_eq!(page["mimeType"], APP_MIME);
    assert!(
        page["text"]
            .as_str()
            .unwrap()
            .contains(r#"<script id="onebyone-data" type="application/json">"#)
    );
    assert!(
        client
            .request(
                "resources/read",
                json!({ "uri": "ui://onebyone/nope.html" })
            )
            .is_err()
    );
}

#[test]
fn get_changes_then_show_walkthrough() {
    let repo = TempRepo::new();
    repo.write("src/a.ts", "const a = 1;\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "init"]);
    repo.write("src/a.ts", "const a = 2;\n");
    repo.write("src/b.ts", "const b = 1;\n");
    let dir = repo.dir.to_string_lossy().into_owned();
    let (mut client, _) = Client::start();

    // 还没切分改动就提交步骤
    let r = client.call(
        "show_walkthrough",
        json!({ "repo": dir, "title": "测试", "steps": [{ "title": "概览" }] }),
    );
    assert_eq!(r["isError"], true);
    assert!(
        text_of(&r).contains("请先调用 get_changes"),
        "{}",
        text_of(&r)
    );

    let r = client.call("get_changes", json!({ "repo": dir }));
    assert_ne!(r["isError"], true, "{}", text_of(&r));
    let listing = text_of(&r);
    let ids: Vec<String> = read_json(&repo.path(".onebyone/hunks.json"))["hunks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids.len(), 2);
    for id in &ids {
        assert!(listing.contains(&format!("### {id}")), "清单里应该有 {id}");
    }
    // 写法要求换成了 MCP 的版本：步骤作为参数提交，不写 steps.json
    assert!(listing.contains("调用 `show_walkthrough` 提交"));
    assert!(!listing.contains("{{"));
    assert_eq!(
        fs::read_to_string(repo.path(".onebyone/hunks.md")).unwrap(),
        listing
    );

    // 只讲了一个改动块：照样生成，同时列出没讲到的
    let r = client.call(
        "show_walkthrough",
        json!({ "repo": dir, "title": "测试", "steps": [{ "title": "只讲一个", "hunks": [ids[0]] }] }),
    );
    let report = text_of(&r);
    assert!(report.contains("还有 1 个改动块没有讲到"), "{report}");
    assert!(report.contains(&ids[1]));
    assert!(report.contains("再调用一次 show_walkthrough"));
    assert_eq!(r["structuredContent"]["report"], report.as_str());
    assert_eq!(
        r["structuredContent"]["walkthrough"]["hunks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let full = json!({
        "repo": dir,
        "title": "测试",
        "summary": "一句话概括",
        "steps": [
            { "title": "概览", "explain": "先看整体" },
            { "id": "all", "kind": "modify", "title": "全部改动", "hunks": ids },
        ],
    });
    let r = client.call("show_walkthrough", full.clone());
    let report = text_of(&r);
    assert!(report.contains("讲解覆盖了全部改动块"), "{report}");
    let data = &r["structuredContent"];
    assert_eq!(data["walkthrough"]["title"], "测试");
    assert_eq!(data["walkthrough"]["summary"], "一句话概括");
    assert_eq!(data["walkthrough"]["steps"][1]["hunks"], json!(ids));
    // 也生成了浏览器能打开的页面，和命令行可以重新 build 的 steps.json
    let page = fs::read_to_string(data["page"].as_str().unwrap()).unwrap();
    assert!(page.contains(&format!("\"id\":\"{}\"", ids[0])));
    let steps = read_json(&repo.path(".onebyone/steps.json"));
    assert_eq!(steps["steps"][1]["kind"], "modify");
    assert!(
        steps["steps"][0].get("chapter").is_none(),
        "没填的字段不写进 steps.json"
    );

    // 切分之后代码又改了：提示重新调用 get_changes
    repo.write("src/a.ts", "const a = 3;\n");
    let r = client.call("show_walkthrough", full);
    assert!(
        text_of(&r).contains("调用 get_changes 之后改动又有变化"),
        "{}",
        text_of(&r)
    );
}

#[test]
fn errors_explain_what_to_fix() {
    let repo = TempRepo::new();
    let (mut client, _) = Client::start();

    let r = client.call("get_changes", json!({ "repo": repo.dir.to_string_lossy() }));
    assert!(text_of(&r).contains("没有发现改动"), "{}", text_of(&r));

    let missing = repo.path("missing");
    let r = client.call("get_changes", json!({ "repo": missing.to_string_lossy() }));
    assert_eq!(r["isError"], true);
    assert!(text_of(&r).contains("找不到目录"), "{}", text_of(&r));

    let r = client.call("get_changes", json!({ "repo": " " }));
    assert!(text_of(&r).contains("缺少 repo"), "{}", text_of(&r));

    // 缺少必填参数时也是工具错误，AI 能看到缺了什么并改正
    let r = client.call("show_walkthrough", json!({ "repo": "." }));
    assert_eq!(r["isError"], true);
    assert!(
        text_of(&r).contains("missing field `title`"),
        "{}",
        text_of(&r)
    );
}

#[test]
fn long_listing_stays_in_the_file() {
    let repo = TempRepo::new();
    // 60 个新文件，每个 30 行：完整清单远超直接放进工具结果的长度
    for i in 0..60 {
        let body: String = (0..30)
            .map(|n| format!("export const value_{i}_{n} = {n};\n"))
            .collect();
        repo.write(&format!("src/file_{i:02}.ts"), body);
    }
    let (mut client, _) = Client::start();
    let r = client.call("get_changes", json!({ "repo": repo.dir.to_string_lossy() }));
    let overview = text_of(&r);
    assert!(overview.contains("改动较多"), "{overview}");
    assert!(overview.contains(".onebyone/hunks.md"));
    assert!(overview.contains("- src/file_59.ts（新增，1 个改动块，+30 −0）"));
    assert!(overview.contains("调用 `show_walkthrough` 提交"));
    assert!(!overview.contains("value_0_0"), "概要里不放代码");
    let listing = fs::read_to_string(repo.path(".onebyone/hunks.md")).unwrap();
    assert!(listing.contains("value_0_0") && listing.len() > overview.len());
}
