# ACoder

> 全流程自主编码引擎 — Autonomous Full-Pipeline Coding Engine

ACoder 是一个基于 Rust 构建的 AI 代码编辑器，支持全场景代码生成、多角色智能体协作、自进化记忆系统与远程控制能力。

## 架构

```
Core Pipeline:  llm → context → planning → execution → agents → memory
Intelligence:   intelligence (自进化引擎)
Platform:       gateway / api / session / sprint / editor / tui
Cross-cutting:  security / observability / skill
```

## 开发阶段

- **Phase 1** — 全场景代码生成 ✅
- **Phase 2** — 编辑器集成 ✅
- **Phase 3** — Memory 系统 ✅
- **Phase 4** — 多角色智能体 ✅
- **Phase 5** — 工程规范 Superpowers + 远程控制 🚧
- **Phase 6** — 自进化 + 公测 🚧

## 构建

```bash
cargo build
cargo run --bin acode
```

## License

MIT OR Apache-2.0
