# StormView (native)

StormView is a native Rust desktop app for Linux and Windows. It has live LibreWXR radar, Open-Meteo and NWS forecasts, NWS warning polygons, air quality, and native alert notifications. It is built in Material 3 Expressive, and every color comes from Google's Material You dynamic-color algorithm.

There is no webview and no browser engine. The release build is a single ~34 MB binary that only needs fontconfig and freetype. It renders on the GPU with OpenGL.

## Stack

| Piece | Library |
| --- | --- |
| UI toolkit | [Slint](https://slint.dev) 1.18: native, GPU-rendered (femtovg / OpenGL), with the winit backend |
| Components | [Slint's official Material 3 component library](https://material.slint.dev) (`material-1.1.0/`) |
| Color | [`material-colors`](https://crates.io/crates/material-colors), a Rust port of Google's material-color-utilities. It uses the spec 2025 schemes (Material 3 Expressive color) and harmonization |
| Shapes | The M3 Expressive shape library (cookies, sunny, burst, flower, clover…), generated as paths in `src/shapes.rs` |
| Type & icons | Google Sans Flex (static instances cut at full roundness) and Material Symbols Rounded |
| Map | A custom slippy-map engine (`src/map.rs`). It draws Esri Light Gray Canvas tiles repainted with the current M3 color roles, LibreWXR radar between the land and the labels, alert polygons rasterized with tiny-skia, and an IDW air-quality overlay |
| Data | reqwest + tokio, using Open-Meteo, NWS, Nominatim, Open-Meteo Air Quality / Seasonal, and LibreWXR |
| Notifications | notify-rust (native OS notifications) |
| Assistant | Ollama or any OpenAI-compatible local server, with streamed responses |

## Why it's smooth

Every map layer sits in one container. Dragging only moves that container's offset, and tiles are re-laid out only when you pan past a pre-loaded margin or change zoom. Tile decoding and recoloring run on worker threads. Radar frames around the current frame are preloaded, and the previous frame stays on screen until the next one arrives, so playback doesn't flicker.

## Build & run

```bash
cargo run --release              # run
./packaging/install.sh           # install to ~/.local (binary, icon, .desktop entry)
```

On **Windows**, install Rust and run `cargo build --release`. The binary is `target\release\stormview.exe`.

Live-network tests: `cargo test -- --ignored --nocapture`

Preview the animated weather scene for any condition with `STORMVIEW_SCENE=<WMO code>[,night]`, for example `STORMVIEW_SCENE=95 cargo run` (thunderstorm) or `STORMVIEW_SCENE=2,night cargo run`. You can turn the scene off in Settings → Display → Layout.

## Keyboard

`Ctrl+,` or `F2` opens settings, `F3` moves to the next settings tab, `Space` plays or pauses the radar, `←` / `→` step through radar frames, `+` / `-` zoom, and `Esc` closes panels. Right-click the map for the context menu.

## Layout

- `src/main.rs`: app controller (callbacks, polling, radar playback, theme, notifications, assistant)
- `src/map.rs`: map engine
- `src/present.rs`: weather data → display values for every widget
- `src/scene.rs` + `ui/scene.slint`: the animated weather scene behind the temperature
- `src/theme.rs`: Material You scheme generation and semantic-color harmonization
- `src/weather.rs`, `src/services.rs`, `src/assistant.rs`: data providers
- `ui/*.slint`: the interface. `main.slint` holds the window layout, `widgets.slint` the home-screen sections, `settings.slint` the settings sheet, `dialogs.slint` the dialogs, and `scrubber.slint` the radar timeline

`legacy-tauri/` holds the earlier webview-based attempt. It isn't used and can be deleted.
