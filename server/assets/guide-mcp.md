---

## 写法要求

请把上面的改动块编排成讲解步骤，调用 `show_walkthrough` 提交。**只写步骤和讲解，不要复制代码**：播放器会按你引用的改动块 id 显示真实的代码。

### show_walkthrough 的参数

```json
{
  "repo": "和调用 get_changes 时相同的项目目录",
  "title": "一句话标题，例如：Todo 支持截止日期",
  "summary": "一两句话说明这次改动做了什么、为什么做",
  "steps": [
    {
      "id": "overview",
      "chapter": "概览",
      "title": "先看整体：这次改了什么",
      "hunks": [],
      "explain": "改动的目标、整体思路和建议的阅读顺序。"
    },
    {
      "id": "types",
      "chapter": "数据层",
      "kind": "add",
      "title": "Todo 类型新增可选字段 dueDate",
      "hunks": ["h3f2a9c"],
      "related": ["date"],
      "explain": "为什么这样改、有什么取舍、读代码时要注意什么。"
    }
  ]
}
```

{{STEPS}}

### 提交之后

`show_walkthrough` 会检查引用的 id 是否都存在、是否覆盖了全部改动块。结果里列出没讲到的改动块或数据问题时，修改步骤后再调用一次，直到显示"讲解覆盖了全部改动块"。

支持 MCP Apps 的客户端会在对话里显示播放器；不能显示时，把结果里的播放页面路径告诉用户，用浏览器打开。
