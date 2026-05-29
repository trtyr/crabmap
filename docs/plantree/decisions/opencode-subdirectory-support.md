# Decision: opencode 支持 agent 和 skill 子目录

**Date**: 2026-05-29
**Status**: Accepted

## Context

opencode 官方文档中 agent 路径写的是 `~/.config/opencode/agent(s)/<name>.md`，没有明确说明是否支持子目录。skill 路径写的是 `**/SKILL.md`，暗示递归扫描。需要验证 agent 是否也支持子目录。

## Decision

经测试，opencode **同时支持** agent 和 skill 的子目录组织：

- **Agent**: `~/.config/opencode/agents/<subdir>/<name>.md` — 可以加载
- **Skill**: `~/.config/opencode/skills/<subdir>/<name>/SKILL.md` — 可以加载

## Verified Structure

```
~/.config/opencode/agents/
├── system/          orchestrator, general, fixer, explorer, librarian
├── dev/             backend, frontend, designer, env-engineer, oracle
├── sec/             cybersec-engineer, redops, js-hunter
└── sillytavern.md

~/.config/opencode/skills/
├── system/          retro
├── dev/
│   ├── rust/        rust-onboard, rust-debug, rust-refactor, ...
│   ├── python/
│   ├── code-review/
│   ├── simplify/
│   └── ...
├── frontend/
├── plan-tree/
├── sec/
└── ...
```

## Implications

- Agent 和 skill 可以按领域分类到子目录，不会影响加载
- 文件名仍然是 agent/skill 的标识符，与目录层级无关
- 可以用子目录来组织大量 agent/skill，提高可维护性
