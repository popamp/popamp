<h1 align="center">Popamp</h1>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · <b>한국어</b>
</p>

<p align="center">
  <a href="#설치"><img src="assets/readme/hero.png" alt="Popamp와 세 가지 스킨: Default, Steel, Modern" width="100%"></a>
</p>

<p align="center">
  <a href="https://github.com/popamp/popamp/actions/workflows/ci.yml"><img src="https://github.com/popamp/popamp/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-8a5cf6" alt="Linux, Windows, macOS">
  <a href="#라이선스"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="MIT OR Apache-2.0"></a>
</p>

<p align="center">
  <b><a href="#설치">다운로드</a></b> ·
  <a href="https://popamp.app/ko/">한국어 사이트</a> ·
  <a href="https://popamp.app/skins">스킨</a> ·
  <a href="https://popamp.app/plugins">플러그인</a>
</p>

> 이 문서는 [영어 README](README.md)의 핵심 부분을 AI로 번역한 것입니다.
> 전체 문서는 영어로 되어 있습니다. 어색한 부분은 issue나 PR로 알려 주세요.

Popamp는 Winamp 2 스타일의 데스크톱 음악 플레이어입니다. 메인 창, 10밴드
이퀄라이저, 재생목록이 서로 붙고, 원할 때마다 모습을 바꿀 수 있습니다. Rust로
만들었고 Linux와 Windows에서 실행되며, macOS 버전은 실험 단계입니다. 플레이어의
메뉴와 대화 상자는 한국어로 나옵니다.

- **어떤 크기에서도 선명하게.** Popamp 자체 스킨은 벡터로 그려서, ×1부터 ×3까지
  0.25 단위로 키워도 어떤 모니터에서든 선명합니다.
- **수천 개의 클래식 스킨.** 아무 `.wsz` 파일이나 열거나, 플레이어를 떠나지 않고
  [Winamp 스킨 뮤지엄](https://skins.webamp.org)에서 스킨을 둘러보고, 검색하고,
  설치하세요.
- **플레이어 안의 갤러리.** [popamp.app](https://popamp.app)의 스킨과 플러그인을
  클릭 한 번으로 설치하고 업데이트합니다. 각 항목은 카탈로그와 대조해 확인합니다.
- **제대로 된 소리.** 갭리스 재생, ReplayGain, `.eqf` 프리셋을 지원하는 10밴드
  이퀄라이저, 인터넷 라디오, 그리고 흔히 쓰는 형식: MP3, AAC, FLAC, Ogg Vorbis,
  Opus, WAV, ALAC, MIDI, 트래커 모듈 등.
- **데스크톱과 잘 어울리게.** 미디어 키를 지원하고, 시스템의 플레이어 컨트롤에
  곡과 커버가 표시됩니다.
- **막대가 되는 플레이어.** [툴바 모드](#툴바-모드)에서는 Linux와 Windows에서
  메인 창이 화면 위나 아래의 막대가 되고, 작업 표시줄처럼 자리를 확보합니다.
- **내가 허락한 플러그인만.** 플러그인은 WebAssembly 샌드박스에서 실행되며, 하나하나
  직접 켜야 합니다: 음원으로 쓰는 [YouTube](https://github.com/popamp/plugin-youtube),
  헤드폰용 [Crossfeed](https://github.com/popamp/plugin-crossfeed), `.ape`와 `.wv`
  파일을 위한 [Monkey's Audio](https://github.com/popamp/plugin-ape)와
  [WavPack](https://github.com/popamp/plugin-wavpack).
- **내 음악은 내 것.** 계정도, 클라우드도, 가져올 라이브러리도 없습니다. 내가 연
  파일과 스트림을 재생할 뿐입니다.

### 스킨

| [Default](https://github.com/popamp/popamp-skin-default) (내장) | [Steel](https://github.com/popamp/skin-steel) | [Modern](https://github.com/popamp/skin-modern) |
|:---:|:---:|:---:|
| <img src="assets/screenshots/default.png" width="250" alt="Popamp 기본 스킨"> | <img src="assets/screenshots/steel.png" width="250" alt="Steel 스킨"> | <img src="assets/screenshots/modern.png" width="250" alt="Modern 스킨"> |

### 툴바 모드

플레이어 전체가 화면 아래나 위의 막대 하나에 들어갑니다. 재생 버튼, 시각화, 시계,
제목, 볼륨, 이퀄라이저와 재생목록 스위치가 들어가고, 이퀄라이저와 재생목록은 막대
오른쪽 끝 위에 놓입니다. Linux(X11)에서는 작업 표시줄처럼 자리를 확보해서 최대화한
창이 그 앞에서 멈추고, 데스크톱 자체의 패널도 가리지 않습니다. Windows에서는
애플리케이션 바가 됩니다. 메인 메뉴 › *툴바 모드*에서 켜고, 옮기고, 크기를 바꿉니다.
내장 스킨과 Modern, Steel, Classified, D11 스킨에는 모두 막대 모양이 있습니다.

<img src="assets/screenshots/bar.png" alt="Modern 스킨을 쓴 툴바 모드: 화면 아래의 막대와 그 오른쪽 끝 위의 이퀄라이저와 재생목록">

## 설치

설치 파일은 [Releases 페이지](https://github.com/popamp/popamp/releases)에 있고,
Debian, Ubuntu, openSUSE, Fedora에서는 저장소에서 설치해 자동으로 업데이트할 수
있습니다. 두 가지 빌드가 있습니다: **popamp**는 플레이어만, **popamp-plus**는
YouTube와 시각화 같은 [플러그인](docs/plugins.md)(영어)이 들어 있는 플레이어입니다.

| 시스템 | 다운로드 |
|---|---|
| Debian, Ubuntu | [apt 저장소](docs/building.md#debian-and-ubuntu)(영어)를 추가한 뒤 `sudo apt install popamp`; 또는 `popamp-<버전>-linux-amd64.deb`(ARM64: `-arm64.deb`)를 내려받아 `sudo apt install ./popamp-…deb` |
| openSUSE, Fedora | [저장소](docs/building.md#opensuse-and-fedora)(영어)를 추가한 뒤 `sudo zypper install popamp` 또는 `sudo dnf install popamp` |
| Arch, Manjaro, EndeavourOS | 릴리스의 `popamp-bin.PKGBUILD`를 빈 폴더에 `PKGBUILD`로 저장하고 `makepkg -si` 실행([방법](docs/building.md#arch-manjaro-endeavouros), 영어) |
| 그 밖의 Linux | `Popamp-<버전>-x86_64.AppImage`: `chmod +x` 후 실행 |
| Windows 10, 11 | `Popamp-<버전>-windows-x86_64-setup.exe`, 또는 포터블 `…-portable.zip`: 아무 곳에나 풀고 `popamp.exe` 실행 |
| macOS 12 이상(실험적) | `Popamp-<버전>-macos-universal.dmg`, Apple Silicon과 Intel용: 열어서 Popamp를 '응용 프로그램'으로 끌어다 놓으세요. 처음 실행할 때는 [시스템 설정에서 한 번 클릭](docs/building.md#macos)(영어)해야 합니다 |

Rust 1.96 이상으로 직접 빌드할 수도 있습니다:

```sh
git clone https://github.com/popamp/popamp.git && cd popamp
cargo build --release                      # popamp-plus는 --features plus 추가
./target/release/popamp ~/Music
```

### 더 보기(영어)

- [기능](docs/features.md): Popamp가 하는 모든 것과 아직 못 하는 것.
- [사용법](docs/usage.md): 단축키, 명령줄 원격 제어, 설정 파일 위치.
- [빌드와 설치](docs/building.md): 요구 사항, 배포판별 패키지, 소스에서 설치.
- [플러그인](docs/plugins.md)과 [스킨 형식](docs/skin-format.md): 직접 만들어 보세요.
- [기여하기](CONTRIBUTING.md), [변경 기록](CHANGELOG.md), [로드맵](docs/ROADMAP.md).

## 라이선스

Popamp는 자유 소프트웨어이며, [MIT](LICENSE-MIT) 또는
[Apache 2.0](LICENSE-APACHE) 라이선스 중 하나를 골라 쓸 수 있습니다. 기여도 같은
조건으로 받습니다. 서드파티 크레이트는 각자의 라이선스를 따르며(Symphonia 디코더는
MPL-2.0), Linamp와 Ferramp에서 가져온 코드는 [THIRD_PARTY.md](THIRD_PARTY.md)에
정리되어 있습니다.

Popamp에는 Winamp의 코드나 그림이 들어 있지 않습니다. Winamp는 해당 소유자의
상표이며, Popamp는 독립 프로젝트로서 그와 제휴하거나 승인을 받지 않았습니다.
