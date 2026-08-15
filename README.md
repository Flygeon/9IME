# 9IME

9IME 是一个全新编写的 Windows 输入法（Rust）。与 Sogou 输入法皮肤 (.ssf)
完全兼容，可导入搜狗皮肤并直接渲染使用。

## 架构

- **9ime-tsf**（DLL）: TSF 文本服务，负责接收按键并把提交文本写入应用。
- **9ime-server**（EXE）: 独立进程，持有 librime 引擎（通过 C ABI 动态加载，
  无 C++ 代码）与候选窗口（GDI，皮肤渲染）。
- **9ime-deployer**（EXE）: egui 图形界面，皮肤导入/选择/删除、重新部署。
- **9ime-ipc** / **9ime-core**: 命名管道协议与皮肤/容器解析（zip 与加密 "Skin"
  格式，AES-256-CBC + zlib 自定义归档）。
- **9ime-console**: 引擎冒烟测试工具。

双进程设计: 键盘事件经命名管道发给 server，server 单线程驱动 librime
（librime 会话必须单线程），并绘制候选窗。

## 构建

需要 Rust stable (x86_64-pc-windows-msvc) 与 Visual Studio Build Tools。
本地无完整工具链时可只用 `cargo check` 验证语法，正式构建走 GitHub Actions。

```
cargo build --release --workspace
```

把 librime 的 rime.dll 与 data/ 放在可执行文件旁边（CI 自动下载）。

## 安装 / 测试

1. 运行安装包或手动注册: `regsvr32 nineime_tsf.dll`（管理员）。
2. 在系统设置中添加 "9IME" 输入法（中文简体内置语言档案）。
3. 打开 nineime-deployer.exe 导入 .ssf 皮肤（或使用 assets/sample.ssf）。
4. 在任意编辑器输入拼音，候选窗应跟随光标显示。

## 皮肤支持

支持搜狗 .ssf 两种容器（普通 zip 与加密格式）、skin.ini 的 UTF-16LE /
UTF-8 / GBK 编码、九宫格拉伸布局 (layout_horizontal / layout_vertical)、
pinyin_marge / zhongwen_marge 排版、颜色与字体。

## 许可

GPL-3.0
