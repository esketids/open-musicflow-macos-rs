# Open MusicFlow Studio — Native Rust Edition (`open-musicflow-macos-rs`)

A **100% pure-Rust, zero-dependency, single-binary rewrite** of **Open MusicFlow Studio** for **macOS** (Universal Apple Silicon `arm64` + Intel `x86_64` `.app` & `.dmg`), **Linux** (`x86_64`), and **Windows** (`x64`).

Unlike the Electron/Node.js edition, this Rust rewrite compiles the entire application — including the FIPS-197 AES-256-CBC cipher engine, LG SJ8 TCP 9741 protocol client & simulator, RFC 6455 WebSocket server, UPnP/DLNA DMS audio synthesizer (`:49252`), and the embedded Studio UI — into a **single ~800 KB native executable** with **zero Node.js, Electron, or external crate dependencies**.

---

## Why the Rust Rewrite?

- **Zero Runtime Dependencies**: No Node.js, `npm`, `node_modules`, or Electron required on your Mac or Linux PC.
- **Single ~800 KB Native Binary**: Embeds the entire UI (`assets/index.html`, `assets/app.css`, `assets/app.js`) at compile time via `include_bytes!`.
- **Native macOS Universal `.app` & `.dmg`**: Runs natively on both Apple Silicon (`M1/M2/M3/M4`) and Intel Macs (`Open MusicFlow Studio.app/Contents/MacOS/OpenMusicFlowStudio`).
- **Instant `< 5ms` Startup & Automatic Live SJ8 Probing**: Starts the HTTP/WebSocket server (`:4741`) and DMS server (`:49252`) in under 5 milliseconds while probing `192.168.0.10:9741` in the background to auto-connect to your physical **LG SJ8** soundbar.

---

## Pure-Rust Architecture (`src/`)

- **[`src/crypto.rs`](src/crypto.rs)**:
  - Cleanroom FIPS-197 **AES-256-CBC** block cipher + PKCS#7 padding in pure Rust.
  - Supports **LG SJ8 V1 Cipher** (`4efgvbn m546Uy7kolKrftgbn =-0u&~` / IV `54eRty@hkL,;/y9U`), **Temescal V2 Cipher** (`T^&*J%^7tr~4^%^&I(o%^!jIJ__+a0 k` / IV `'%^Ur7gy$~t+f)%@`), and **`0x00` Plaintext** framing.
  - Pure-Rust **SHA-1 + Base64** for RFC 6455 WebSocket handshakes (`Sec-WebSocket-Accept`).
- **[`src/json.rs`](src/json.rs)**:
  - Zero-dependency recursive JSON parser and serializer (`JsonValue`).
- **[`src/simulator.rs`](src/simulator.rs)**:
  - Built-in Virtual **LG SJ8** TCP 9741 Hardware Simulator (`Sound Bar SJ8`, `NB9.504.00629.C`, `wooferoffset: -15`, `ALARM_SET` modes `0/2/3/4`, `SET_ALARM_PLAYLIST`).
- **[`src/protocol.rs`](src/protocol.rs)**:
  - Thread-safe LG SJ8 TCP 9741 client, UDP SSDP multicast discovery (`239.255.255.250:1900`), `SHARE_HOME_INFO` (`192.168.5.100:9741`) Wi-Fi provisioning, `SPK_ADD_SET` speaker registration, and real-time 16-bit 44.1 kHz PCM WAV audio synthesizer.
- **[`src/main.rs`](src/main.rs)**:
  - Multithreaded HTTP REST API + RFC 6455 WebSocket (`/ws`) server on `:4741`, UPnP/DLNA DMS server on `:49252` (`/dmr.xml` & `/stream/:track`), and native desktop window launcher.

---

## Building & Running from Source

```bash
cargo test
cargo build --release
./target/release/open-musicflow-rs
```

To package into `Open MusicFlow Studio.app` on macOS:
```bash
bash scripts/package-macos-app.sh
open "release/Open MusicFlow Studio.app"
```
