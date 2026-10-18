//! MCP 服务：让 Codex、Claude 等 AI 客户端直接调用 OneByOne，通过标准输入输出通信。
//!
//! - `get_changes`：切分改动，返回带编号的改动清单和写法要求
//! - `show_walkthrough`：提交讲解步骤，检查覆盖率，生成讲解。支持 MCP Apps 的客户端会用界面资源
//!   `ui://onebyone/player.html` 在对话里显示播放器，讲解数据放在工具结果的 `structuredContent` 里
//!
//! 标准输出只能写协议消息，这里的代码不能打印任何内容。

use crate::{
    diff::{self, CollectOptions, OUT_DIR},
    listing::{self, Target},
    page,
    util::{read_json, to_posix, write_json, write_text},
    workflow::{self, HUNKS_FILE, LISTING_FILE, STEPS_FILE},
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ExtensionCapabilities, Implementation, JsonObject,
        ListResourcesResult, MetaObject, PaginatedRequestParams, ReadResourceRequestParams,
        ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
        ServerConfig,
    },
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

/// 播放器界面的资源地址，show_walkthrough 通过 `_meta.ui.resourceUri` 指向它
pub const PLAYER_URI: &str = "ui://onebyone/player.html";
/// MCP Apps 规定的界面资源类型
pub const APP_MIME: &str = "text/html;profile=mcp-app";
const UI_EXTENSION: &str = "io.modelcontextprotocol/ui";
const INSTRUCTIONS: &str = include_str!("../assets/mcp-instructions.md");
// 清单超过这个长度时，结果里只放概要，让 AI 分段读 hunks.md，免得超出客户端对工具结果的长度限制
const INLINE_LISTING_MAX_CHARS: usize = 30_000;

#[derive(Deserialize, schemars::JsonSchema)]
struct GetChangesArgs {
    /// 项目目录的绝对路径（git 仓库或其中的子目录）
    repo: String,
    /// 对比基准，默认 HEAD。想把 AI 这次做的几个提交也算进来时，填这些提交之前的那个提交
    #[serde(default)]
    base: String,
    /// 对比终点的提交；不填表示对比到工作区，包括未提交和未跟踪的文件
    #[serde(default)]
    head: String,
    /// 只看这些路径（相对 repo）；不填表示全部
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ShowWalkthroughArgs {
    /// 和调用 get_changes 时相同的项目目录
    repo: String,
    /// 一句话标题，例如：Todo 支持截止日期
    title: String,
    /// 一两句话说明这次改动做了什么、为什么做
    #[serde(default)]
    summary: String,
    /// 讲解步骤，按理解顺序排列，第一步写概览
    steps: Vec<StepArgs>,
}

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[schemars(inline)]
struct StepArgs {
    /// 步骤 id，给 related 引用
    #[serde(default, skip_serializing_if = "String::is_empty")]
    id: String,
    /// 章节名，同一章节的步骤放在一起
    #[serde(default, skip_serializing_if = "String::is_empty")]
    chapter: String,
    /// 类型：add、modify、delete、refactor、fix、test、config、docs、style
    #[serde(default, skip_serializing_if = "String::is_empty")]
    kind: String,
    /// 步骤标题，一句话说清这一步做了什么
    title: String,
    /// 引用的改动块 id（get_changes 清单里的编号），可以跨多个文件；概览步骤留空
    #[serde(default)]
    hunks: Vec<String>,
    /// 关联步骤的 id
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    related: Vec<String>,
    /// 讲解：为什么这样改、有什么取舍、要注意什么。支持简单 Markdown
    #[serde(default, skip_serializing_if = "String::is_empty")]
    explain: String,
}

struct OneByOne {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl OneByOne {
    /// 切分项目里的代码改动，返回带编号的改动清单，末尾附有编排讲解步骤的写法要求。
    /// 用户想看懂这次改了什么、要求讲解改动时先调用它，再按要求调用 show_walkthrough。
    #[tool(
        title = "切分代码改动",
        annotations(
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_changes(&self, Parameters(args): Parameters<GetChangesArgs>) -> CallToolResult {
        run_blocking(move || get_changes(&args)).await
    }

    /// 提交讲解步骤：检查是否引用了不存在的改动块、是否覆盖了全部改动，生成可以逐步播放的讲解。
    /// 需要先调用 get_changes。支持的客户端会在对话里显示播放器。
    #[tool(
        title = "播放改动讲解",
        meta = player_meta(),
        annotations(destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn show_walkthrough(
        &self,
        Parameters(args): Parameters<ShowWalkthroughArgs>,
    ) -> CallToolResult {
        run_blocking(move || show_walkthrough(&args)).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for OneByOne {
    fn get_info(&self) -> ServerConfig {
        let mut ui = JsonObject::new();
        ui.insert("mimeTypes".to_string(), json!([APP_MIME]));
        let mut extensions = ExtensionCapabilities::new();
        extensions.insert(UI_EXTENSION.to_string(), ui);
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_extensions_with(extensions)
                .build(),
        )
        .with_server_info(
            Implementation::new("onebyone", env!("CARGO_PKG_VERSION")).with_title("OneByOne"),
        )
        .with_instructions(INSTRUCTIONS.trim_end())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(PLAYER_URI, "player")
                .with_title("OneByOne 播放器")
                .with_description("逐步播放代码改动和讲解")
                .with_mime_type(APP_MIME),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if request.uri != PLAYER_URI {
            return Err(ErrorData::resource_not_found(
                format!("没有这个资源：{}", request.uri),
                None,
            ));
        }
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(page::player(), PLAYER_URI).with_mime_type(APP_MIME),
        ])
        .into())
    }
}

/// 启动 MCP 服务，一直运行到客户端断开连接
pub fn run() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("启动 MCP 服务失败：{e}"))?;
    runtime.block_on(async {
        let server = OneByOne {
            tool_router: OneByOne::tool_router(),
        };
        let service = server
            .serve(stdio())
            .await
            .map_err(|e| format!("MCP 连接初始化失败：{e}"))?;
        service
            .waiting()
            .await
            .map_err(|e| format!("MCP 服务异常退出：{e}"))?;
        Ok(())
    })
}

/// show_walkthrough 的 `_meta`：告诉客户端用哪个界面显示结果。
/// 同时写上已经弃用的扁平写法 `ui/resourceUri`，兼容只认它的客户端（官方 SDK 也是这样做的）
fn player_meta() -> MetaObject {
    let mut meta = MetaObject::new();
    meta.insert("ui".to_string(), json!({ "resourceUri": PLAYER_URI }));
    meta.insert("ui/resourceUri".to_string(), json!(PLAYER_URI));
    meta
}

/// git 和读写文件都会阻塞，放到单独的线程里做，期间照样能响应客户端的其他请求。
/// 出错时返回工具级错误：AI 能看到错误信息，按提示改正后重试
async fn run_blocking<F>(task: F) -> CallToolResult
where
    F: FnOnce() -> Result<CallToolResult, String> + Send + 'static,
{
    let outcome = tokio::task::spawn_blocking(task)
        .await
        .unwrap_or_else(|e| Err(format!("执行失败：{e}")));
    outcome.unwrap_or_else(|msg| {
        CallToolResult::error(vec![ContentBlock::text(format!("错误：{msg}"))])
    })
}

fn get_changes(args: &GetChangesArgs) -> Result<CallToolResult, String> {
    let repo = repo_dir(&args.repo)?;
    let diff = workflow::collect(&CollectOptions {
        cwd: &repo,
        base: non_empty(&args.base).unwrap_or("HEAD"),
        head: non_empty(&args.head),
        paths: &args.paths,
    })?;
    let range = workflow::range_label(&diff);
    if diff.hunks.is_empty() {
        return Ok(text(format!("没有发现改动（{range}）。")));
    }
    let listing_path = diff.root.join(OUT_DIR).join(LISTING_FILE);
    let full = listing::render(&diff, &range, &Target::Mcp);
    write_text(&listing_path, &full)?;
    Ok(text(if full.chars().count() <= INLINE_LISTING_MAX_CHARS {
        full
    } else {
        listing::render_overview(&diff, &range, &to_posix(&listing_path))
    }))
}

fn show_walkthrough(args: &ShowWalkthroughArgs) -> Result<CallToolResult, String> {
    let root = diff::repo_root(&repo_dir(&args.repo)?)?;
    let out_dir = root.join(OUT_DIR);
    let hunks_file = out_dir.join(HUNKS_FILE);
    if !hunks_file.exists() {
        return Err("还没有切分这个项目的改动，请先调用 get_changes".to_string());
    }
    let saved = read_json(&hunks_file, &root)?;
    let steps = json!({ "title": args.title, "summary": args.summary, "steps": args.steps });
    // 同时写一份 steps.json，之后可以用命令行重新 build，或者对照排查
    write_json(&out_dir.join(STEPS_FILE), &steps)?;
    let built = workflow::build(&root, &saved, &steps, "steps")?;

    let page_path = to_posix(&built.page_path);
    let mut lines = vec![format!("讲解：{}", args.title)];
    lines.extend(built.report_lines(&Target::Mcp));
    lines.push(String::new());
    lines.push(format!("播放页面：{page_path}"));
    lines.push(if built.is_clean() {
        "支持 MCP Apps 的客户端会在对话里显示播放器；不能显示时，请把播放页面的路径告诉用户，用浏览器打开。"
    } else {
        "请按上面的提示修改步骤，再调用一次 show_walkthrough。"
    }
    .to_string());
    let report = lines.join("\n");

    let mut result = text(report.clone());
    // 按 MCP Apps 规范，structuredContent 只交给界面、不进模型上下文；
    // 但有的客户端（例如 Codex）会把它交给模型，所以检查结果也放一份在最前面
    result.structured_content = Some(json!({
        "report": report,
        "page": page_path,
        "walkthrough": built.walkthrough,
    }));
    Ok(result)
}

fn text(message: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(message)])
}

/// AI 传来的项目目录。客户端启动服务时的当前目录不一定是项目目录，所以要求填绝对路径；
/// 相对路径仍按当前目录解析
fn repo_dir(repo: &str) -> Result<PathBuf, String> {
    let repo = repo.trim();
    if repo.is_empty() {
        return Err("缺少 repo：请填写项目目录的绝对路径".to_string());
    }
    let dir = std::env::current_dir()
        .map(|cwd| cwd.join(repo))
        .unwrap_or_else(|_| PathBuf::from(repo));
    // 先检查目录，否则 git 会因为目录不存在而启动失败，报出"找不到 git 命令"这种误导的错误
    if !dir.is_dir() {
        return Err(format!("找不到目录：{repo}"));
    }
    Ok(dir)
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}
