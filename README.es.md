<h1 align="center">Popamp</h1>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ko.md">한국어</a> · <b>Español</b> · <a href="README.pt-BR.md">Português (Brasil)</a>
</p>

<p align="center">
  <a href="#instalación"><img src="assets/readme/hero.png" alt="Popamp con tres de sus skins: Default, Steel y Modern" width="100%"></a>
</p>

<p align="center">
  <a href="https://github.com/popamp/popamp/actions/workflows/ci.yml"><img src="https://github.com/popamp/popamp/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-8a5cf6" alt="Linux, Windows y macOS">
  <a href="#licencia"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="MIT OR Apache-2.0"></a>
</p>

<p align="center">
  <b><a href="#instalación">Descargar</a></b> ·
  <a href="https://popamp.app/es/">Sitio en español</a> ·
  <a href="https://popamp.app/es/skins">Skins</a> ·
  <a href="https://popamp.app/es/plugins">Plugins</a>
</p>

> Este documento es una traducción con IA de las partes principales del
> [README en inglés](README.md). La documentación completa está en inglés.
> Si algo suena raro, avísanos con un issue o un PR.

Popamp es un reproductor de música de escritorio al estilo de Winamp 2: ventana
principal, ecualizador de 10 bandas y lista de reproducción que se acoplan entre
sí, y un aspecto nuevo cuando quieras. Está escrito en Rust y funciona en Linux y
Windows, con una versión experimental para macOS. Los menús y diálogos del
reproductor están en español.

- **Nítido a cualquier tamaño.** Los skins propios de Popamp están dibujados como
  vectores, así que se ven nítidos de ×1 a ×3 en pasos de un cuarto, en cualquier
  monitor.
- **Miles de skins clásicos.** Abre cualquier archivo `.wsz`, o explora, busca e
  instala skins del [Winamp Skin Museum](https://skins.webamp.org) sin salir del
  reproductor.
- **Una galería dentro del reproductor.** Instala y actualiza los skins y plugins
  de [popamp.app](https://popamp.app) con un clic; cada uno se comprueba contra el
  catálogo.
- **Suena bien.** Reproducción sin pausas, ReplayGain, ecualizador de 10 bandas
  con presets `.eqf`, radio por internet y los formatos habituales: MP3, AAC,
  FLAC, Ogg Vorbis, Opus, WAV, ALAC, MIDI, módulos de tracker y más.
- **Como en casa en tu escritorio.** Teclas multimedia, y los controles de
  reproducción del sistema muestran la pista y su carátula.
- **Un reproductor que es una barra.** En [modo barra](#modo-barra) la ventana
  principal se convierte en una franja en la parte superior o inferior de la
  pantalla, en Linux y Windows, con su espacio reservado como el de una barra de
  tareas.
- **Plugins que tú autorizas.** Plugins WebAssembly aislados, cada uno activado
  por ti: [YouTube](https://github.com/popamp/plugin-youtube) como fuente,
  [Crossfeed](https://github.com/popamp/plugin-crossfeed) para auriculares,
  [Monkey's Audio](https://github.com/popamp/plugin-ape) y
  [WavPack](https://github.com/popamp/plugin-wavpack) para archivos `.ape` y `.wv`.
- **Tu música sigue siendo tuya.** Sin cuenta, sin nube, sin biblioteca que
  importar: reproduce los archivos y streams que abres.

### Skins

| [Default](https://github.com/popamp/popamp-skin-default) (incorporado) | [Steel](https://github.com/popamp/skin-steel) | [Modern](https://github.com/popamp/skin-modern) |
|:---:|:---:|:---:|
| <img src="assets/screenshots/default.png" width="250" alt="El skin predeterminado de Popamp"> | <img src="assets/screenshots/steel.png" width="250" alt="El skin Steel"> | <img src="assets/screenshots/modern.png" width="250" alt="El skin Modern"> |

### Modo barra

Todo el reproductor en una franja en la parte inferior o superior de la
pantalla: controles, visualizador, reloj, título, volumen y los interruptores del
ecualizador y la lista, con el ecualizador y la lista de reproducción sobre su
extremo derecho. En Linux (X11) la franja queda reservada como una barra de
tareas, así que las ventanas maximizadas se detienen en ella, y no tapa los
paneles del escritorio; en Windows es una barra de aplicación. Se activa, se
mueve y se ajusta en Menú principal › *Modo barra*. El skin incorporado y los
skins Modern, Steel, Classified y D11 tienen barra, en todos sus juegos de
colores.

<img src="assets/screenshots/bar.png" alt="Modo barra con el skin Modern: una franja en la parte inferior de la pantalla con el ecualizador y la lista de reproducción sobre su extremo derecho">

## Instalación

Las descargas están en la [página de versiones](https://github.com/popamp/popamp/releases),
y en Debian, Ubuntu, openSUSE y Fedora se puede instalar desde un repositorio que
lo mantiene actualizado. Hay dos versiones: **popamp**, el reproductor, y
**popamp-plus**, el reproductor con [plugins](docs/plugins.md) (en inglés) como
YouTube y visualizaciones.

| Sistema | Descarga |
|---|---|
| Debian, Ubuntu | agrega el [repositorio apt](docs/building.md#debian-and-ubuntu) (en inglés) y luego `sudo apt install popamp`; o descarga `popamp-<versión>-linux-amd64.deb` (ARM64: `-arm64.deb`) y `sudo apt install ./popamp-…deb` |
| openSUSE, Fedora | agrega el [repositorio](docs/building.md#opensuse-and-fedora) (en inglés) y luego `sudo zypper install popamp` o `sudo dnf install popamp` |
| Arch, Manjaro, EndeavourOS | `popamp-bin.PKGBUILD` de la versión: guárdalo como `PKGBUILD` en una carpeta vacía y ejecuta `makepkg -si` ([cómo](docs/building.md#arch-manjaro-endeavouros), en inglés) |
| Cualquier otro Linux | `Popamp-<versión>-x86_64.AppImage`: dale `chmod +x` y ejecútalo |
| Windows 10 u 11 | `Popamp-<versión>-windows-x86_64-setup.exe`, o el portable `…-portable.zip`: descomprímelo donde quieras y ejecuta `popamp.exe` |
| macOS 12 o posterior (experimental) | `Popamp-<versión>-macos-universal.dmg`, para Apple Silicon e Intel: ábrelo y arrastra Popamp a Aplicaciones; el primer inicio necesita [un clic en Ajustes del Sistema](docs/building.md#macos) (en inglés) |

O compílalo tú mismo con Rust 1.96 o posterior:

```sh
git clone https://github.com/popamp/popamp.git && cd popamp
cargo build --release                      # agrega --features plus para popamp-plus
./target/release/popamp ~/Music
```

### Más información (en inglés)

- [Funciones](docs/features.md): todo lo que hace Popamp, y lo que todavía no hace.
- [Uso](docs/usage.md): teclas, control remoto desde la línea de comandos, dónde se guarda la configuración.
- [Compilar e instalar](docs/building.md): requisitos, paquetes por distribución, instalar desde el código fuente.
- [Plugins](docs/plugins.md) y [el formato de skins](docs/skin-format.md), para hacer los tuyos.
- [Cómo contribuir](CONTRIBUTING.md), [Cambios](CHANGELOG.md), [Hoja de ruta](docs/ROADMAP.md).

## Apoyo

Popamp es gratis, sin anuncios, sin rastreo y sin funciones de pago. Si te gusta,
puedes [invitar al proyecto a un café en Ko-fi](https://ko-fi.com/popamp): sirve
para pagar el alojamiento, el hardware de pruebas y las versiones firmadas.

## Licencia

Popamp es software libre bajo la licencia [MIT](LICENSE-MIT) o
[Apache 2.0](LICENSE-APACHE), a tu elección; las contribuciones se aceptan bajo
los mismos términos. Las crates de terceros conservan sus propias licencias (los
decodificadores de Symphonia son MPL-2.0), y el código tomado de Linamp y Ferramp
aparece en [THIRD_PARTY.md](THIRD_PARTY.md).

Popamp no contiene código ni gráficos de Winamp. Winamp es una marca de su
propietario; Popamp es un proyecto independiente y no está afiliado a él ni
cuenta con su respaldo.
