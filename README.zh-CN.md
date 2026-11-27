<h1 align="center">Popamp</h1>

<p align="center">
  <a href="README.md">English</a> · <b>简体中文</b> · <a href="README.ko.md">한국어</a> · <a href="README.es.md">Español</a> · <a href="README.pt-BR.md">Português (Brasil)</a>
</p>

<p align="center">
  <a href="#安装"><img src="assets/readme/hero.png" alt="Popamp 和它的三款皮肤：Default、Steel 和 Modern" width="100%"></a>
</p>

<p align="center">
  <a href="https://github.com/popamp/popamp/actions/workflows/ci.yml"><img src="https://github.com/popamp/popamp/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-8a5cf6" alt="Linux、Windows 和 macOS">
  <a href="#许可证"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="MIT OR Apache-2.0"></a>
</p>

<p align="center">
  <b><a href="#安装">下载</a></b> ·
  <a href="https://popamp.app/zh/">中文网站</a> ·
  <a href="https://popamp.app/skins">皮肤</a> ·
  <a href="https://popamp.app/plugins">插件</a>
</p>

> 本文由 AI 从[英文版 README](README.md) 翻译而来，只包含最重要的部分。
> 完整文档为英文。翻译有误之处，欢迎提 issue 或 PR 指正。

Popamp 是一款 Winamp 2 风格的桌面音乐播放器：主窗口、10 段均衡器和播放列表
可以吸附在一起，想换个样子随时可以换。它用 Rust 编写，支持 Linux 和 Windows，
macOS 版处于实验阶段。播放器的菜单和对话框已有中文界面。

- **任何尺寸都清晰。** Popamp 自己的皮肤是矢量绘制的，从 ×1 到 ×3（以 0.25
  为步长）在任何显示器上都清晰锐利。
- **成千上万款经典皮肤。** 可以打开任意 `.wsz` 文件，也可以不离开播放器，
  直接在 [Winamp Skin Museum](https://skins.webamp.org) 中浏览、搜索和安装皮肤。
- **播放器内置图库。** 一键安装和更新 [popamp.app](https://popamp.app)
  上的皮肤和插件，每一项都会与目录核对。
- **声音到位。** 无缝播放、ReplayGain、支持 `.eqf` 预设的 10 段均衡器、
  网络电台，以及常见格式：MP3、AAC、FLAC、Ogg Vorbis、Opus、WAV、ALAC、MIDI、
  音轨模块（tracker）等。
- **融入你的桌面。** 支持媒体键，系统的播放控件会显示当前曲目和封面。
- **播放器也能变成一条栏。** 在[工具栏模式](#工具栏模式)下，主窗口变成屏幕
  顶部或底部的一条栏（Linux 和 Windows），并像任务栏一样占住自己的位置。
- **插件由你授权。** 插件在 WebAssembly 沙盒中运行，每一个都需要你亲自开启：
  [YouTube](https://github.com/popamp/plugin-youtube) 音源、
  适合耳机的 [Crossfeed](https://github.com/popamp/plugin-crossfeed)，以及播放
  `.ape` 和 `.wv` 文件的 [Monkey's Audio](https://github.com/popamp/plugin-ape)
  和 [WavPack](https://github.com/popamp/plugin-wavpack)。
- **你的音乐只属于你。** 无需账号，没有云端，也不用导入音乐库：它只播放你打开
  的文件和网络流。

### 皮肤

| [Default](https://github.com/popamp/popamp-skin-default)（内置） | [Steel](https://github.com/popamp/skin-steel) | [Modern](https://github.com/popamp/skin-modern) |
|:---:|:---:|:---:|
| <img src="assets/screenshots/default.png" width="250" alt="Popamp 的默认皮肤"> | <img src="assets/screenshots/steel.png" width="250" alt="Steel 皮肤"> | <img src="assets/screenshots/modern.png" width="250" alt="Modern 皮肤"> |

### 工具栏模式

整个播放器收进屏幕底部或顶部的一条栏里：播放控制、可视化、时钟、标题、音量，
以及均衡器和播放列表的开关；均衡器和播放列表显示在栏的右端上方。在 Linux（X11）
上，这条栏会像任务栏一样占住位置，最大化的窗口会停在它旁边，也不会挡住桌面自带
的面板；在 Windows 上它是一个应用程序栏。在 主菜单 › *工具栏模式* 中开启、移动
和调整大小。内置皮肤以及 Modern、Steel、Classified 和 D11 皮肤都带有工具栏样式。

<img src="assets/screenshots/bar.png" alt="使用 Modern 皮肤的工具栏模式：屏幕底部的一条栏，均衡器和播放列表在它右端上方">

## 安装

安装包在 [Releases 页面](https://github.com/popamp/popamp/releases)；Debian、
Ubuntu、openSUSE 和 Fedora 也可以从软件源安装并自动更新。每种都有两个版本：
**popamp** 是播放器本身，**popamp-plus** 是带有 YouTube、可视化等
[插件](docs/plugins.md)（英文）的播放器。

| 系统 | 下载 |
|---|---|
| Debian、Ubuntu | 添加 [apt 软件源](docs/building.md#debian-and-ubuntu)（英文），然后 `sudo apt install popamp`；或下载 `popamp-<版本>-linux-amd64.deb`（ARM64：`-arm64.deb`），再 `sudo apt install ./popamp-…deb` |
| openSUSE、Fedora | 添加[软件源](docs/building.md#opensuse-and-fedora)（英文），然后 `sudo zypper install popamp` 或 `sudo dnf install popamp` |
| Arch、Manjaro、EndeavourOS | 从发布页下载 `popamp-bin.PKGBUILD`，在空文件夹中保存为 `PKGBUILD`，运行 `makepkg -si`（[说明](docs/building.md#arch-manjaro-endeavouros)，英文） |
| 装有 Flatpak 的 Linux | `flatpak install --user https://apt.popamp.app/popamp.flatpakref`（popamp-plus，x86_64；[详情](docs/building.md#flatpak)） |
| 其他 Linux | `Popamp-<版本>-x86_64.AppImage`：`chmod +x` 后直接运行 |
| Windows 10 或 11 | `Popamp-<版本>-windows-x86_64-setup.exe`，或便携版 `…-portable.zip`：解压到任意位置，运行 `popamp.exe` |
| macOS 12 或更高（实验性） | `Popamp-<版本>-macos-universal.dmg`，支持 Apple Silicon 和 Intel：打开后把 Popamp 拖到“应用程序”；首次启动需要[在系统设置中点一下](docs/building.md#macos)（英文） |

也可以用 Rust 1.96 或更高版本自行编译：

```sh
git clone https://github.com/popamp/popamp.git && cd popamp
cargo build --release                      # 加上 --features plus 即为 popamp-plus
./target/release/popamp ~/Music
```

### 更多文档（英文）

- [功能](docs/features.md)：Popamp 能做的一切，以及暂时还不能做的。
- [使用](docs/usage.md)：快捷键、命令行遥控、设置文件的位置。
- [编译与安装](docs/building.md)：依赖、各发行版的软件包、从源码安装。
- [插件](docs/plugins.md)和[皮肤格式](docs/skin-format.md)：制作你自己的插件和皮肤。
- [参与贡献](CONTRIBUTING.md)、[更新日志](CHANGELOG.md)、[路线图](docs/ROADMAP.md)。

## 支持

Popamp 免费，没有广告、没有跟踪、没有付费功能。如果你喜欢它，可以
[在 Ko-fi 上请项目喝杯咖啡](https://ko-fi.com/popamp)：这些钱用于服务器、测试设备和签名版本。

## 许可证

Popamp 是自由软件，你可以任选 [MIT](LICENSE-MIT) 或
[Apache 2.0](LICENSE-APACHE) 许可证；贡献的代码也按同样的条款接收。第三方 crate
保留各自的许可证（Symphonia 解码器为 MPL-2.0），来自 Linamp 和 Ferramp 的代码
列在 [THIRD_PARTY.md](THIRD_PARTY.md) 中。

Popamp 不含任何 Winamp 代码或美术素材。Winamp 是其所有者的商标；Popamp 是独立
项目，与其没有任何关联，也未获其认可。
