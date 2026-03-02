# AntHill

一个基于 Rust 的插件管理系统，提供 Web API 用于插件管理和执行。

A plugin management system built with Rust, providing a Web API for plugin management and execution.

## 功能特性

- 插件安装、更新、卸载管理
- 插件执行和状态跟踪
- RESTful API 接口
- SQLite 数据库存储
- 支持本地 Python 插件
- Windows 系统托盘支持

## 前置要求

- Rust 1.85+ (2024 edition)
- SQLite3

## 安装

```bash
# 克隆仓库
git clone <repository-url>
cd AntHill

# 构建项目
cargo build --release

# 编译后的二进制文件位于 target/release/anthill
```

## 配置

AntHill 可以通过以下方式配置（优先级从低到高）：

### 1. 默认配置

- 数据库: `sqlite:{data_dir}/anthill.db`
- 监听地址: `127.0.0.1:6701`

### 2. 配置文件

创建配置文件 `{config_dir}/config.json`：

```json
{
  "database_url": "sqlite:anthill.db",
  "host": "127.0.0.1",
  "port": 6701,
  "uv_path": "uv"
}
```

### 3. 环境变量

- `DATABASE_URL`: 数据库连接字符串
- `HOST`: 服务器监听地址
- `PORT`: 服务器端口

## 使用方法

### 启动服务器

```bash
# 直接运行
cargo run

# 或使用编译后的二进制文件
./target/release/anthill
```

### API 接口

服务器启动后，可以通过以下 API 端点进行交互：

- `GET /` - 健康检查
- `GET /plugins` - 列出所有插件
- `POST /plugins/install` - 安装新插件
- `DELETE /plugins/:id` - 卸载插件
- `POST /plugins/:id/execute` - 执行插件
- `GET /executions` - 查看执行历史

详细 API 文档请查看源码中的 [api](src/api) 模块。

## 项目结构

```
AntHill/
├── src/
│   ├── main.rs          # 程序入口
│   ├── api/             # API 路由和处理
│   ├── config/          # 配置管理
│   ├── error/           # 错误类型定义
│   ├── executor/        # 插件执行器
│   ├── models/          # 数据模型
│   ├── paths/           # 路径工具
│   ├── repository/      # 数据库访问层
│   └── services/        # 业务逻辑层
├── frontend/            # 前端界面
├── skills/              # 插件开发技能
└── scripts/             # 构建和部署脚本
```

## 开发

### 运行开发服务器

```bash
cargo run
```

### 运行测试

```bash
cargo test
```

### 查看日志

设置 `RUST_LOG` 环境变量来控制日志级别：

```bash
RUST_LOG=debug cargo run
```

## 打包

### 使用打包脚本

项目提供了自动化打包脚本 `scripts/package_bundle.sh`，可以构建包含 uv 和前端静态资源的发布包。

#### 基本用法

```bash
# 为当前平台打包
./scripts/package_bundle.sh

# 指定输出目录
./scripts/package_bundle.sh --output-dir /path/to/dist

# 跳过构建步骤（使用已有的二进制文件）
./scripts/package_bundle.sh --skip-build
```

#### 跨平台打包

```bash
# 为 Windows x86_64 打包（需要 cargo-xwin）
./scripts/package_bundle.sh --target x86_64-pc-windows-msvc

# 为 Linux aarch64 打包
./scripts/package_bundle.sh --target aarch64-unknown-linux-gnu

# 为 macOS ARM64 打包
./scripts/package_bundle.sh --target aarch64-apple-darwin
```

#### 自定义 uv 版本

```bash
# 指定 uv 版本
./scripts/package_bundle.sh --uv-version 0.9.26

# 使用自定义 uv 下载 URL
./scripts/package_bundle.sh --uv-url https://example.com/custom-uv.zip
```

### 打包输出

脚本会在 `dist/` 目录下生成：

```
dist/
└── anthill-0.1.3-linux-x86_64/
    ├── anthill              # 可执行文件
    ├── web/                 # 打包后的前端静态资源
    ├── bin/
    │   └── uv               # uv 二进制文件
    ├── conf/
    │   └── config.json      # 默认配置
    ├── VERSION              # 版本信息
    └── data/                # 数据目录（运行时创建）
```

同时会生成一个 `.zip` 压缩包，方便分发。运行该发布包时不需要 Bun/Node.js 等 JS 运行时：

```
dist/anthill-0.1.3-linux-x86_64.zip
```

### 安装打包版本

解压后直接运行即可：

```bash
unzip anthill-0.1.3-linux-x86_64.zip
cd anthill-0.1.3-linux-x86_64
./anthill
```
