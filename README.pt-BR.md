<h1 align="center">Popamp</h1>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ko.md">한국어</a> · <a href="README.es.md">Español</a> · <b>Português (Brasil)</b>
</p>

<p align="center">
  <a href="#instalação"><img src="assets/readme/hero.png" alt="Popamp com três de suas skins: Default, Steel e Modern" width="100%"></a>
</p>

<p align="center">
  <a href="https://github.com/popamp/popamp/actions/workflows/ci.yml"><img src="https://github.com/popamp/popamp/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-8a5cf6" alt="Linux, Windows e macOS">
  <a href="#licença"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="MIT OR Apache-2.0"></a>
</p>

<p align="center">
  <b><a href="#instalação">Baixar</a></b> ·
  <a href="https://popamp.app/pt-br/">Site em português</a> ·
  <a href="https://popamp.app/pt-br/skins">Skins</a> ·
  <a href="https://popamp.app/pt-br/plugins">Plug-ins</a>
</p>

> Este documento é uma tradução feita por IA das partes principais do
> [README em inglês](README.md). A documentação completa está em inglês.
> Se algo soar estranho, avise com uma issue ou um PR.

O Popamp é um player de música para desktop no espírito do Winamp 2: janela
principal, equalizador de 10 bandas e playlist que se encaixam, e um visual novo
sempre que você quiser. Ele é escrito em Rust e roda no Linux e no Windows, com
uma versão experimental para macOS. Os menus e as janelas do player estão em
português.

- **Nítido em qualquer tamanho.** As skins próprias do Popamp são desenhadas como
  vetores, então ficam nítidas de ×1 a ×3 em passos de um quarto, em qualquer
  monitor.
- **Milhares de skins clássicas.** Abra qualquer arquivo `.wsz`, ou navegue, busque
  e instale skins do [Winamp Skin Museum](https://skins.webamp.org) sem sair do
  player.
- **Uma galeria dentro do player.** Instale e atualize as skins e plug-ins do
  [popamp.app](https://popamp.app) com um clique; cada um é conferido com o
  catálogo.
- **Som do jeito certo.** Reprodução sem pausas, ReplayGain, equalizador de 10
  bandas com presets `.eqf`, rádio pela internet e os formatos comuns: MP3, AAC,
  FLAC, Ogg Vorbis, Opus, WAV, ALAC, MIDI, módulos de tracker e mais.
- **Em casa no seu desktop.** Teclas de mídia, e os controles de mídia do sistema
  mostram a faixa e a capa.
- **Um player que é uma barra.** No [modo barra](#modo-barra) a janela principal
  vira uma faixa no topo ou na base da tela, no Linux e no Windows, com o espaço
  reservado como o de uma barra de tarefas.
- **Plug-ins que você autoriza.** Plug-ins WebAssembly isolados, cada um ativado
  por você: [YouTube](https://github.com/popamp/plugin-youtube) como fonte,
  [Crossfeed](https://github.com/popamp/plugin-crossfeed) para fones de ouvido,
  [Monkey's Audio](https://github.com/popamp/plugin-ape) e
  [WavPack](https://github.com/popamp/plugin-wavpack) para arquivos `.ape` e `.wv`.
- **Sua música continua sua.** Sem conta, sem nuvem, sem biblioteca para
  importar: ele toca os arquivos e streams que você abre.

### Skins

| [Default](https://github.com/popamp/popamp-skin-default) (embutida) | [Steel](https://github.com/popamp/skin-steel) | [Modern](https://github.com/popamp/skin-modern) |
|:---:|:---:|:---:|
| <img src="assets/screenshots/default.png" width="250" alt="A skin padrão do Popamp"> | <img src="assets/screenshots/steel.png" width="250" alt="A skin Steel"> | <img src="assets/screenshots/modern.png" width="250" alt="A skin Modern"> |

### Modo barra

O player inteiro em uma faixa na base ou no topo da tela: controles,
visualizador, relógio, título, volume e os botões do equalizador e da playlist,
com o equalizador e a playlist acima da ponta direita. No Linux (X11) a faixa é
reservada como uma barra de tarefas, então janelas maximizadas param nela, e ela
não cobre os painéis do desktop; no Windows é uma barra de aplicativo. Ative,
mova e ajuste o tamanho em Menu principal › *Modo barra*. A skin embutida e as
skins Modern, Steel, Classified e D11 têm barra, em todos os conjuntos de cores.

<img src="assets/screenshots/bar.png" alt="Modo barra com a skin Modern: uma faixa na base da tela com o equalizador e a playlist acima da ponta direita">

## Instalação

Os downloads estão na [página de versões](https://github.com/popamp/popamp/releases),
e no Debian, Ubuntu, openSUSE e Fedora dá para instalar de um repositório que
mantém o player atualizado. Há duas versões: **popamp**, o player, e
**popamp-plus**, o player com [plug-ins](docs/plugins.md) (em inglês) como
YouTube e visualizações.

| Sistema | Download |
|---|---|
| Debian, Ubuntu | adicione o [repositório apt](docs/building.md#debian-and-ubuntu) (em inglês) e depois `sudo apt install popamp`; ou baixe `popamp-<versão>-linux-amd64.deb` (ARM64: `-arm64.deb`) e `sudo apt install ./popamp-…deb` |
| openSUSE, Fedora | adicione o [repositório](docs/building.md#opensuse-and-fedora) (em inglês) e depois `sudo zypper install popamp` ou `sudo dnf install popamp` |
| Arch, Manjaro, EndeavourOS | `popamp-bin.PKGBUILD` da versão: salve como `PKGBUILD` em uma pasta vazia e rode `makepkg -si` ([como](docs/building.md#arch-manjaro-endeavouros), em inglês) |
| Qualquer outro Linux | `Popamp-<versão>-x86_64.AppImage`: dê `chmod +x` e rode |
| Windows 10 ou 11 | `Popamp-<versão>-windows-x86_64-setup.exe`, ou o portátil `…-portable.zip`: descompacte onde quiser e rode `popamp.exe` |
| macOS 12 ou mais recente (experimental) | `Popamp-<versão>-macos-universal.dmg`, para Apple Silicon e Intel: abra e arraste o Popamp para Aplicativos; a primeira abertura precisa de [um clique nos Ajustes do Sistema](docs/building.md#macos) (em inglês) |

Ou compile você mesmo com Rust 1.96 ou mais recente:

```sh
git clone https://github.com/popamp/popamp.git && cd popamp
cargo build --release                      # adicione --features plus para popamp-plus
./target/release/popamp ~/Music
```

### Mais informações (em inglês)

- [Recursos](docs/features.md): tudo o que o Popamp faz, e o que ainda não faz.
- [Uso](docs/usage.md): teclas, controle remoto pela linha de comando, onde ficam as configurações.
- [Compilar e instalar](docs/building.md): requisitos, pacotes por distribuição, instalar a partir do código-fonte.
- [Plug-ins](docs/plugins.md) e [o formato de skins](docs/skin-format.md), para criar os seus.
- [Como contribuir](CONTRIBUTING.md), [Alterações](CHANGELOG.md), [Roteiro](docs/ROADMAP.md).

## Apoio

O Popamp é gratuito, sem anúncios, sem rastreamento e sem recursos pagos. Se você
gostar, pode [pagar um café para o projeto no Ko-fi](https://ko-fi.com/popamp):
isso ajuda a pagar a hospedagem, o hardware de testes e as versões assinadas.

## Licença

O Popamp é software livre sob a licença [MIT](LICENSE-MIT) ou
[Apache 2.0](LICENSE-APACHE), à sua escolha; contribuições são aceitas nos mesmos
termos. Crates de terceiros mantêm suas próprias licenças (os decodificadores do
Symphonia são MPL-2.0), e o código vindo do Linamp e do Ferramp está listado em
[THIRD_PARTY.md](THIRD_PARTY.md).

O Popamp não contém código nem arte do Winamp. Winamp é uma marca registrada de
seu proprietário; o Popamp é um projeto independente e não é afiliado a ele nem
endossado por ele.
