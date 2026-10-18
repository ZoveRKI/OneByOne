//! OneByOne：把一次代码改动拆成有顺序的步骤，每一步显示对应的 diff 和讲解。
//!
//! 流程：`hunks`（切分改动，写出给 AI 读的清单）→ AI 写 `.onebyone/steps.json`
//! → `build`（合并、检查覆盖率，生成播放页面）→ `open`（在浏览器里打开）

pub mod cli;
pub mod diff;
pub mod listing;
pub mod model;
pub mod page;
pub mod util;
pub mod workflow;
