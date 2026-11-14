# Vector skin format

A vector skin is drawn in SVG, so it stays sharp at every window size,
including fractional ones such as x1.25 or x1.5. Popamp's own default skin
is a vector skin:
[popamp-skin-default](https://github.com/popamp/popamp-skin-default) is a
complete example that draws every classic sheet. Skins that describe their own windows
live in repositories of their own:
[Popamp Modern](https://github.com/popamp/skin-modern) (round buttons,
rounded windows), [Popamp Steel](https://github.com/popamp/skin-steel)
(own pixel fonts, a shade mode for every window) and
[Popamp Bar](https://github.com/popamp/skin-bar) (a main window that is
a bar across the screen).

A vector skin is a folder, or a `.psz` archive (a zip file under another
name), with a file named `skin.toml` at its top. Opening a `.psz` file in
Popamp installs and applies it; a folder or `.zip` placed in the skins
folder works too, and so does a skin cloned there with git (hidden files
such as `.git` are ignored). Everything else in it (SVG pictures, TrueType
fonts) is referred to from `skin.toml` by its path inside the skin, for
example `"buttons/play.svg"`.

Coordinates are in **skin pixels**: the classic main window is 275 x 116 of
them, whatever size the window is shown at. An SVG drawn 23 units wide
covers 23 skin pixels.

While a skin is in use, Popamp checks its files twice a second and loads
it again after you save a change. If the change breaks the skin, the
current look stays and the title display shows the error.

## Two ways to build a skin

1. **Draw the classic sheets in SVG** (`[sheets]`). You get the classic
   layout of all three windows and their shade modes, with every button,
   slider and text, and only draw the pictures. This is the easy way.
2. **Describe your own windows** (`[[window]]`). You place every element
   yourself and can give windows any size and set of controls.

Both can be mixed: windows you describe replace the classic ones, the rest
keep the classic layout.

## A minimal skin

```toml
[meta]
name = "My Skin"
author = "Me"
format_version = 1

[sheets]
main = "main.svg"
cbuttons = "cbuttons.svg"
```

Sheets you leave out, and anything else missing, come from Popamp's
default skin.

## `[meta]`

| Key | | |
|---|---|---|
| `name` | required | Shown in the skins menu. |
| `author` | optional | |
| `format_version` | required | `1`. |
| `min_popamp` | optional | The oldest Popamp the skin works with, such as `"0.8.0"`. An older one says it needs a newer Popamp instead of loading the skin. |

Set it when the skin uses a key a later Popamp brought: older players
refuse a key they do not know, with a less helpful message. It is
read first, so later players give that message even when the rest of the
file uses keys they have never seen. Popamp 0.8.0 is the first that reads
it; a skin with it does not load in older ones at all.

## `[sheets]`: SVG drawings of the classic sheets

Each key is a classic skin bitmap without `.bmp`; the value is an SVG
drawing of it at the same size and with the same sprite positions as the
classic bitmap:

`main`, `cbuttons`, `titlebar`, `shufrep`, `text`, `volume`, `balance`,
`monoster`, `playpaus`, `numbers`, `nums_ex`, `posbar`, `eqmain`, `eq_ex`,
`pledit`.

`nums_ex` (digits with a minus sign) is optional: when a skin has it, the
play time uses it instead of `numbers`.

For example `cbuttons.svg` is 136 x 36 units with the Previous button's
normal picture at 0, 0, 23 x 18 and its pressed picture at 0, 18. Any
reference on classic Winamp 2 skins describes where each sprite is.

## `[shapes]`: window shapes for the classic layout

The opaque part of the picture is the visible window; transparent parts
let the desktop show through. Keys are `main`, `eq`, `playlist`, and the
same with `_shade` for the shade modes:

```toml
[shapes]
main = "shape-main.svg"
```

## `[text]` and `[clock]`: fonts

Without these, the small texts (song title, kbps, kHz...) use the `text`
sheet and the play time uses the `numbers` sheet. With them, they are
written in a TrueType font:

```toml
[text]
font = "fonts/display.ttf"   # optional; the system's sans-serif font otherwise
size = 7                     # height in skin pixels
color = "#00e000"

[clock]
font = "fonts/digits.ttf"
size = 13
color = "#00e000"
```

Characters the font does not have (for example Japanese in a song title)
are taken from a system font that has them.

## `[playlist]`

```toml
[playlist]
font = "fonts/list.ttf"      # optional
normal = "#00ff00"           # text
current = "#ffffff"          # the playing entry
normal_bg = "#000000"
selected_bg = "#0000c6"
```

Colours left out keep the classic ones. The text size is the user's
playlist font size setting.

## `[vis]`

```toml
[vis]
colors = ["#000000", "#181818", ...]   # 24 colours
```

The order is the classic `viscolor.txt` one: background, background dots,
16 spectrum colours from the top of a bar down, 5 oscilloscope colours,
the peak dots.

## `[colors]`: colour sets

A skin can come in several colourings, which the user picks in the main
menu's "Colours". Each is a set of named colours:

```toml
[colors.night]            # the first set is the one a new user sees
back = "#0d1720"
accent = "#8fd6ff"

[colors.rose]
accent = "#ff7ab8"        # `back` stays as in the first set
```

The first set names every colour; the others list only the ones they
change. A set's name is shown in the menu as written, so
`[colors."Dark blue"]` works. A colour's name is a plain word: letters,
digits, `-` and `_`.

In a picture, an element takes a colour by carrying its name as a class:

```svg
<rect class="back" width="275" height="116" fill="#0d1720"/>
<path class="accent-stroke" d="M3 8h20" stroke="#8fd6ff"/>
<stop class="accent-stop" offset="0" stop-color="#8fd6ff"/>
```

`class="accent"` sets the fill, `class="accent-stroke"` the line colour
and `class="accent-stop"` a gradient stop's colour. Keep the usual `fill`
or `stroke` as well: it is what drawing programs show, and what Popamp
draws for an element without a class. A class on a group colours what is
in it, unless an element there has a colour of its own.

Wherever `skin.toml` takes a colour (`[text]`, `[clock]`, `[playlist]`,
`[vis]`), a name works too:

```toml
[text]
size = 7
color = "accent"
```

Colour sets came in Popamp 1.0.0, so a skin with them sets
`min_popamp = "1.0.0"`.

## `[[window]]`: your own windows

```toml
[[window]]
key = "main"                 # main, eq, playlist or library
placement = "free"           # free (moves the docked group), docked,
                             # bar-top or bar-bottom (main window only)
empty_press = "move"         # nothing, move or move-if-easy
wheel_volume = 8             # volume change per mouse wheel step
# wheel_scrolls_list = true  # the wheel scrolls the list instead

[window.normal]
size = [275, 116]
# resize_step = [25, 29]     # makes the window resizable in these steps
cursor = "normal"            # the default mouse cursor (see Words)
shape = "shape.svg"          # optional window shape, stretched over the window
# see_through = true         # or: the window shows only where its images,
                             # list, vis and EQ graph are; right for windows
                             # that resize

[[window.normal.element]]
type = "image"
rect = [0, 0, 275, 116]
normal = "main/background.svg"

[[window.normal.element]]
type = "button"
rect = [39, 88, 23, 18]
normal = "main/play.svg"
pressed = "main/play-down.svg"
click = "play"
shift = "open-file"
tip = "Play"
```

A main window placed `bar-top` or `bar-bottom` is a bar: a strip as wide
as the screen at that edge, inside the desktop's own panels, which other
windows keep out of like a taskbar. Its `size` gives the height and the narrowest width; place its
elements with `edges` so they follow the width. The playlist and the
equalizer sit against the bar at the screen's right end and do not move;
the main menu's "Toolbar mode" > "Position" puts the bar at the other edge. A bar has
its own size, set in "Toolbar mode" > "Size" and ×2 at first, apart
from the other windows' size: a bar 14 skin pixels high is 28 screen
pixels high.

The `library` window is the media library. Its frame is the skin's; one
`library` element is the rectangle the player lays the library out in.
Each of its lists has its own scroll bar along its right side, as wide as
the element's `thumb` picture, which is the bar's thumb (`pressed` while
it is dragged); without a `thumb` the player draws narrow bars of its
own. The window needs no `list-scroll`. A skin without a `library` window gets one made of its playlist's
pictures. It came in Popamp 1.0.0.

`[window.shade]` describes the shade mode the same way. Elements are drawn
in the order they are listed; the mouse goes to the last one under it.

### A window that can also be a bar

A main window that is a window can have a bar view as well. The user
switches between the two in the main menu's "Toolbar mode" (On or Off), so one skin
serves both:

```toml
[[window]]
key = "main"

[window.normal]
size = [275, 116]
# ...

[window.bar]
edge = "bottom"              # top or bottom; bottom when left out
size = [640, 14]             # the narrowest width, and the height

[[window.bar.element]]
type = "image"
edges = [0, 0, "end", 14]
normal = "bar/body.svg"
fill = "tile"
```

`[window.bar]` is described like `[window.normal]`, and what is said above
about a bar holds for it. While the bar is shown, the window's shade mode
is not used, and a press on an empty spot does not move it.

A skin that draws the classic sheets can add a bar to the classic main
window: a `[[window]]` with `key = "main"` and `[window.bar]` alone, without
`[window.normal]`. The bar's pictures can be areas of the sheets. A skin without `[window.bar]` has no such menu. Bar views
came in Popamp 1.0.0, so a skin with one sets `min_popamp = "1.0.0"`.

### Where an element is

- `rect = [x, y, width, height]` for an element that stays put, or
- `edges = [left, top, right, bottom]` for one that follows a resizable
  window. Each edge is a number (from the window's left or top), `"end-20"`
  (20 from its right or bottom) or `"center+5"` (5 past its middle).

### Pictures

A picture is an SVG file, drawn at its own size from the element's
top-left corner:

```toml
normal = "play.svg"
```

or an area of a bigger SVG, optionally shifted:

```toml
normal = { file = "buttons.svg", area = [23, 0, 23, 18], offset = [0, 1] }
```

### Element types

Every element can also have `show`, `hide` or `min_width` (when it is
visible: while a player state holds, while it does not, or while the
window is at least that wide), `cursor`, `tip` (tooltip text), `menu`
(its right-click menu) and `hit`: a picture whose opaque part is where the
element takes the mouse, for round buttons. Without `hit` the whole
rectangle does:

```toml
normal = "play.svg"
hit = "play.svg"       # only the round part is the button
```

| `type` | What it is | Its keys |
|---|---|---|
| `image` | A picture | `normal`, `on` (while `lit` holds), `inactive` (window not focused), `fill` (`once` or `tile`), `lit` |
| `drag` | Moves the window | `double` (command on double click) |
| `button` | A push button | `normal`, `pressed`, `inactive`, and what it does |
| `toggle` | A button showing a state | `normal`, `pressed`, `on`, `on_pressed`, `flag`, and what it does |
| `slider` | A slider | `value`, `vertical`, `track`, `thumb`, `thumb_len`, `live`, `reset`, `wheel_step`, `detent` |
| `text` | A text | `text` |
| `marquee` | The scrolling song title | `area` (the text's place inside the element; all of it when left out), `double` |
| `clock` | The play time | `align` (`left`, `center` or `right`: where a `[clock]` font's time sits across the element; `left` when left out), `font_size` |
| `vis` | Spectrum analyzer or oscilloscope | |
| `eq-graph` | The equalizer curve | |
| `list` | The playlist's track list | |
| `list-scroll` | The playlist's scroll bar | `thumb` |
| `library` | The media library's content: views, search, artists, albums, track table and buttons, in the playlist's font and `[playlist]` colours, with a scroll bar for each list | `thumb` |
| `popup` | A button opening a column of picture buttons | `item` (each with `normal`, `pressed` and what it does), `bar`, `above` |
| `resize` | Resizes the window | |

What a button does is one of:

- `click = "command"`, run when the mouse button is released over it;
  `shift` and `ctrl` give commands for Shift+click and Ctrl+click;
- `press = "command"`, run as soon as the button goes down;
- `menu_press = "menu"` or `menu_release = "menu"`, opening a menu.

A popup's items stack upward, the first one over the button, as in
Winamp. A click leaves the column open; so does holding the button and
letting go on an item, which runs that item. With `above = true` the items
start above the button and leave it in sight.

A clock written in a `[clock]` font is centred from top to bottom on its
digits. With `align = "right"` it ends at the element's right edge, so
it keeps against what is drawn there whatever the font (the system's
font differs between Windows and Linux). The classic digit sheets have
fixed places and ignore `align`. It came in Popamp 0.8.0, so a skin with
it sets `min_popamp = "0.8.0"`.

`font_size` gives this clock its own font height where `[clock]`'s `size`
does not fit, as in a bar view lower than the window's display. It came in
Popamp 1.0.0.

A slider:

```toml
[[window.normal.element]]
type = "slider"
rect = [107, 57, 68, 13]
value = "volume"
track = { file = "volume.svg", first = [0, 0, 68, 13], step = [0, 15], count = 28 }
thumb = { normal = "thumb.svg", pressed = "thumb-down.svg" }
reset = 200        # value a double click sets
wheel_step = 8     # change per wheel step over the slider
```

Values run from 0 to 255 for `volume`, -127 to 127 for `balance`, 0 to 63
for `preamp` and the EQ bands (31 is the middle), and over the track's
length in milliseconds for `position`.

`track` is one picture per value step in a grid: the first at `first`,
the next `step` further, `count` of them, `per_row` to a row, with `map`
`rising` (first at the minimum), `falling` or `from-middle`. `live = false`
changes the value only when the slider is let go (as the position bar
does). `detent` snaps values that close to the middle to it.

### Words

Commands, states, values, texts, menus and cursors are written as words.
A wrong word is reported with the list of right ones.

- **Commands:** the player's commands in lower case with dashes, for
  example `play`, `pause`, `stop`, `next`, `previous`, `open-file`,
  `toggle-shuffle`, `toggle-repeat`, `toggle-eq-window`,
  `toggle-playlist-window`, `toggle-library-window`, `toggle-windowshade`,
  `minimize`, `exit`.
- **Player states** (`flag`, `lit`, `show`, `hide`): `playing`, `paused`,
  `stopped`, `seekable`, `stereo`, `mono`, `shuffle`, `repeat`,
  `eq-window`, `playlist-window`, `library-window`, `always-on-top`, `double-size`, `eq-on`,
  `eq-autoload`.
- **Values:** `volume`, `balance`, `position`, `preamp`, `eq-band-1` to
  `eq-band-10`.
- **Texts:** `kbps`, `khz`, `short-time`, `title`, `playlist-time`,
  `playlist-clock`, `library-title`, `library-status`.
- **Menus:** `main`, `options`, `vis`, `eq-presets`, `playlist-row`,
  `playlist-misc`, `playlist-sort`, `playlist-remove-misc`, `library`, and the
  button menus `previous`, `play`, `pause`, `stop`, `next`, `eject`,
  `seek`, `repeat`, `shuffle`, `time`, `song-title`.
- **Cursors:** the classic cursor file names without `.cur`: `normal`,
  `close`, `titlebar`, `volbal`, `posbar`, `songname`, `psize`...

## Mistakes

`skin.toml` is checked strictly. An unknown key, a key that does not apply
to an element type, a missing file or a file that is not an SVG or a font
stops the skin from loading, with a message such as:

```
skin.toml, line 12: unknown command `plya`; it can be one of: exit, ...
```

## Tips

- Draw on whole skin pixels where you want sharp edges at x1; at larger
  sizes everything is smooth anyway.
- SVG `<text>` is not drawn. Convert texts in pictures to outlines.
- Keep each SVG's `width` and `height` equal to the area it covers.

## Converting a bitmap skin

Redraw the design with the simplest primitives; do not trace its pixels.
Matching the bitmap pixel for pixel with thousands of tiny shapes is the
wrong goal: such a sheet is heavy and shows seams at fractional scales.

- Draw each element whole: a panel, bar, button or slot is one shape, or a
  few, never a mosaic.
- Draw a gradient as one gradient fill, not as a stack of coloured strips.
- Same-coloured details that cannot be one rectangle, such as letters and
  digits, are outlines in one `<path>`, not separate pixels.
- Do not carry over the bitmap's artifacts. Stretching and scaling in the
  original leave stray pixels, blur, banding and uneven repeats: filter
  them out and draw what the artist meant.
