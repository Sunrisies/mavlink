# 交叉编译指南

本指南介绍如何使用 `cargo-zigbuild` 进行 Rust 项目的交叉编译，特别是针对 ARM64 Linux 平台。

## 为什么选择 cargo-zigbuild

`cargo-zigbuild` 通过 `zig cc` 来交叉编译 C/C++ 代码，Zig 编译器本身就是一个功能强大的交叉编译工具链，无需额外安装针对特定目标的 GCC。这使得交叉编译过程更加简单和可靠。

## 安装步骤

### 1. 安装 Zig

在 Windows 系统上使用 winget 安装 Zig：

```bash
winget install Zig.Zig
```

### 2. 安装 cargo-zigbuild

使用 cargo 安装 cargo-zigbuild：

```bash
cargo install cargo-zigbuild
```

### 3. 添加 Rust 目标架构

添加需要交叉编译的目标平台，例如 ARM64 Linux：

```bash
rustup target add aarch64-unknown-linux-gnu
```

## 编译命令

### 基本编译

针对 ARM64 Linux 平台进行 Release 编译：

```bash
cargo zigbuild --target aarch64-unknown-linux-gnu --release
```

### 其他目标平台

你可以为不同的目标平台添加相应的架构：

```bash
# ARMv7 Linux (32位)
rustup target add armv7-unknown-linux-gnueabihf
cargo zigbuild --target armv7-unknown-linux-gnueabihf --release

# x86_64 Linux
rustup target add x86_64-unknown-linux-gnu
cargo zigbuild --target x86_64-unknown-linux-gnu --release
```

## 优化编译选项

在 `Cargo.toml` 中添加以下配置可以减小最终二进制文件的大小：

```toml
[profile.release]
opt-level = "z"     # 优化二进制文件大小
lto = true          # 启用链接时优化
codegen-units = 1   # 减少代码生成单元以提高优化效果
strip = true        # 去除符号信息
```

## 部署

编译完成后，二进制文件位于：

```
target/aarch64-unknown-linux-gnu/release/your_project_name
```

你可以将此文件复制到目标设备上运行。

## 常见问题

### 1. 编译失败

如果编译失败，请确保：
- Zig 已正确安装并添加到 PATH
- cargo-zigbuild 已正确安装
- 目标架构已正确添加到 Rust 工具链

### 2. 运行时错误

如果在目标设备上运行时出现错误，请检查：
- 目标设备的 glibc 版本是否与编译环境兼容
- 是否缺少必要的运行时库

### 3. 静态链接

如果需要完全静态链接，可以添加以下环境变量：

```bash
RUSTFLAGS="-C target-feature=+crt-static" cargo zigbuild --target aarch64-unknown-linux-gnu --release
```
