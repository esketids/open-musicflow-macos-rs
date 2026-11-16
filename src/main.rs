//! Open MusicFlow Studio (macOS / Linux / Windows Native Rust Edition)
//! Single-binary zero-dependency Rust server & desktop launcher with embedded UI,
//! FIPS-197 AES-256-CBC LG SJ8 protocol engine, RFC 6455 WebSockets, and DMS :49252 server.

mod crypto;
mod json;
mod protocol;
mod simulator;

use crate::crypto::{days_list_to_bitmask, websocket_accept_key, EncryptionMode};
use crate::json::{json_str, parse_json};
use crate::protocol::{
    connect_soundbar_tcp, discover_ssdp_multicast, encode_ws_text_frame, probe_soundbar_port,
    send_one_shot_lg_command, synthesize_wav_track, SoundbarController, WifiAlarmEntry,
    LG_DEFAULT_TCP_PORT, LG_DMS_HTTP_PORT,
};
use crate::simulator::start_mock_soundbar_server;
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const INDEX_HTML: &[u8] = include_bytes!("../assets/index.html");
const APP_CSS: &[u8] = include_bytes!("../assets/app.css");
const APP_JS: &[u8] = include_bytes!("../assets/app.js");

const DMR_XML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0" xmlns:dlna="urn:schemas-dlna-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:MediaServer:1</deviceType>
    <friendlyName>Open MusicFlow DMS (Rust Native)</friendlyName>
    <manufacturer>Open MusicFlow Contributors</manufacturer>
    <modelName>LG Music Flow Compatible DMS (Rust)</modelName>
    <UDN>uuid:151B17E8-6B25-4DFE-90BD-6838CC607164</UDN>
  </device>
</root>"#;

fn main() {
    let args: Vec<String> = env::args().collect();
    let headless = args.iter().any(|a| a == "--headless")
        || env::var("NO_BROWSER").map(|v| v == "1").unwrap_or(false);
    let preferred_port: u16 = env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4741);

    let mock_port = start_mock_soundbar_server(19741);
    let controller = Arc::new(Mutex::new(SoundbarController::new()));

    // Connect to built-in Rust simulator immediately (< 2ms), then probe live LG SJ8 in background
    connect_soundbar_tcp(
        &controller,
        "127.0.0.1",
        mock_port,
        EncryptionMode::Sj8V1,
        true,
    );

    let force_sim = env::var("FORCE_SIMULATOR").map(|v| v == "1").unwrap_or(false);
    if !force_sim {
        let ctrl_probe = Arc::clone(&controller);
        thread::spawn(move || {
            if probe_soundbar_port("192.168.0.10", LG_DEFAULT_TCP_PORT, 600) {
                connect_soundbar_tcp(
                    &ctrl_probe,
                    "192.168.0.10",
                    LG_DEFAULT_TCP_PORT,
                    EncryptionMode::Sj8V1,
                    false,
                );
            }
        });
    }

    // Bind main UI & REST/WebSocket server on preferred_port (4741)
    let mut api_port = preferred_port;
    let api_listener = loop {
        match TcpListener::bind(("0.0.0.0", api_port)) {
            Ok(l) => break l,
            Err(_) => api_port += 1,
        }
    };

    // Also bind UPnP/DLNA DMS port 49252 for LG SJ8 SET_ALARM_PLAYLIST & audio streaming
    if let Ok(dms_listener) = TcpListener::bind(("0.0.0.0", LG_DMS_HTTP_PORT)) {
        let ctrl_dms = Arc::clone(&controller);
        thread::spawn(move || {
            for stream in dms_listener.incoming().flatten() {
                let c = Arc::clone(&ctrl_dms);
                thread::spawn(move || handle_http_connection(stream, c, mock_port, api_port));
            }
        });
    }

    println!("================================================================");
    println!("  Open MusicFlow Studio (Native Rust Edition v1.0.0)");
    println!("  Web UI & REST/WS API : http://127.0.0.1:{}", api_port);
    println!("  UPnP / DLNA DMS      : http://0.0.0.0:{}/dmr.xml", LG_DMS_HTTP_PORT);
    println!("  Virtual LG SJ8 Sim   : 127.0.0.1:{} (TCP 9741 AES-256-CBC)", mock_port);
    println!("================================================================");

    if !headless {
        let url = format!("http://127.0.0.1:{}", api_port);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(350));
            open_native_desktop_window(&url);
        });
    }

    for stream in api_listener.incoming().flatten() {
        let c = Arc::clone(&controller);
        thread::spawn(move || handle_http_connection(stream, c, mock_port, api_port));
    }
}

fn open_native_desktop_window(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let chrome_bin = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
        let brave_bin = "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser";
        let edge_bin = "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge";
        if std::path::Path::new(chrome_bin).exists() {
            let _ = Command::new(chrome_bin).arg(format!("--app={}", url)).spawn();
            return;
        } else if std::path::Path::new(brave_bin).exists() {
            let _ = Command::new(brave_bin).arg(format!("--app={}", url)).spawn();
            return;
        } else if std::path::Path::new(edge_bin).exists() {
            let _ = Command::new(edge_bin).arg(format!("--app={}", url)).spawn();
            return;
        }
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
    }
}

fn handle_http_connection(
    mut stream: TcpStream,
    controller: Arc<Mutex<SoundbarController>>,
    mock_port: u16,
    api_port: u16,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = [0u8; 16384];
    let mut total_read = 0usize;
    let header_end = loop {
        match stream.read(&mut buf[total_read..]) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                total_read += n;
                if let Some(pos) = find_subslice(&buf[..total_read], b"\r\n\r\n") {
                    break pos + 4;
                }
                if total_read >= buf.len() {
                    return;
                }
            }
        }
    };

    let header_str = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = header_str.lines();
    let req_line = match lines.next() {
        Some(l) => l,
        None => return,
    };
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/");

    let mut content_len = 0usize;
    let mut ws_key: Option<String> = None;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:") {
            if let Some(val) = line.split(':').nth(1) {
                content_len = val.trim().parse().unwrap_or(0);
            }
        } else if lower.starts_with("sec-websocket-key:") {
            if let Some(val) = line.split(':').nth(1) {
                ws_key = Some(val.trim().to_string());
            }
        }
    }

    // Handle RFC 6455 WebSocket upgrade on /ws
    if path == "/ws" {
        if let Some(key) = ws_key {
            let accept = websocket_accept_key(&key);
            let handshake = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                accept
            );
            if stream.write_all(handshake.as_bytes()).is_ok() {
                let _ = stream.set_read_timeout(None);
                if let Ok(mut guard) = controller.lock() {
                    let init_msg = format!(
                        r#"{{"type":"init","data":{}}}"#,
                        guard.full_api_state_json()
                    );
                    let _ = stream.write_all(&encode_ws_text_frame(&init_msg));
                    guard.add_ws_client(stream);
                }
            }
            return;
        }
    }

    if method == "OPTIONS" {
        send_response(&mut stream, "204 No Content", "text/plain", b"");
        return;
    }

    let mut body_bytes = buf[header_end..total_read].to_vec();
    while body_bytes.len() < content_len {
        let mut chunk = [0u8; 4096];
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => body_bytes.extend_from_slice(&chunk[..n]),
        }
    }
    let body_str = String::from_utf8_lossy(&body_bytes).to_string();
    let body_json = parse_json(&body_str);

    // Static & DMS Routes
    if method == "GET" || method == "HEAD" {
        if path == "/" || path == "/index.html" {
            send_response(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX_HTML);
            return;
        }
        if path.ends_with(".css") {
            send_response(&mut stream, "200 OK", "text/css; charset=utf-8", APP_CSS);
            return;
        }
        if path.ends_with(".js") {
            send_response(
                &mut stream,
                "200 OK",
                "application/javascript; charset=utf-8",
                APP_JS,
            );
            return;
        }
        if path == "/dmr.xml" {
            send_response(
                &mut stream,
                "200 OK",
                "application/xml; charset=utf-8",
                DMR_XML.as_bytes(),
            );
            return;
        }
        if path.starts_with("/stream/") {
            let track_id = path.trim_start_matches("/stream/").trim_end_matches(".wav");
            let wav = synthesize_wav_track(track_id);
            send_response(&mut stream, "200 OK", "audio/wav", &wav);
            return;
        }
        if path == "/api/state" {
            if let Ok(guard) = controller.lock() {
                let json = guard.full_api_state_json();
                send_json(&mut stream, &json);
            }
            return;
        }
        if path == "/api/discover" {
            let mut devs = vec![
                format!(
                    r#"{{"name":"Sound Bar SJ8 (Built-In Rust Simulator)","ip":"127.0.0.1","port":{},"model":"SJ8","isSimulator":true,"verified":true}}"#,
                    mock_port
                ),
                format!(
                    r#"{{"name":"Sound Bar SJ8 (OpenWrt Routed LAN)","ip":"192.168.0.10","port":9741,"model":"SJ8","mac":"7c:1c:4e:3b:b0:22","isSimulator":false,"verified":{}}}"#,
                    probe_soundbar_port("192.168.0.10", 9741, 450)
                ),
                format!(
                    r#"{{"name":"MusicFlow_Setup AP (Wi-Fi Provisioning Mode)","ip":"192.168.5.100","port":9741,"model":"SJ8 Setup AP","isSimulator":false,"verified":{}}}"#,
                    probe_soundbar_port("192.168.5.100", 9741, 350)
                ),
            ];
            for (ip, banner) in discover_ssdp_multicast(800) {
                if ip != "192.168.0.10" {
                    devs.push(format!(
                        r#"{{"name":{},"ip":{},"port":9741,"model":"SSDP MediaRenderer","isSimulator":false,"verified":true}}"#,
                        json_str(&banner),
                        json_str(&ip)
                    ));
                }
            }
            let out = format!(r#"{{"devices":[{}]}}"#, devs.join(","));
            send_json(&mut stream, &out);
            return;
        }
    }

    // POST / DELETE API Routes
    match (method, path) {
        ("POST", "/api/connect") => {
            let is_sim = body_json
                .as_ref()
                .and_then(|b| b.get_bool("isSimulator"))
                .unwrap_or(false);
            let host = if is_sim {
                "127.0.0.1".to_string()
            } else {
                body_json
                    .as_ref()
                    .and_then(|b| b.get_str("host"))
                    .unwrap_or("192.168.0.10")
                    .to_string()
            };
            let port = if is_sim {
                mock_port
            } else {
                body_json
                    .as_ref()
                    .and_then(|b| b.get_i64("port"))
                    .unwrap_or(9741) as u16
            };
            let enc = body_json
                .as_ref()
                .and_then(|b| b.get_str("encryption"))
                .map(EncryptionMode::from_str)
                .unwrap_or(EncryptionMode::Sj8V1);

            let ok = connect_soundbar_tcp(&controller, &host, port, enc, is_sim);
            thread::sleep(Duration::from_millis(80));
            if let Ok(guard) = controller.lock() {
                let resp = format!(
                    r#"{{"ok":{},"soundbar":{}}}"#,
                    ok,
                    guard.soundbar_state_json()
                );
                send_json(&mut stream, &resp);
            }
        }
        ("POST", "/api/disconnect") => {
            if let Ok(mut guard) = controller.lock() {
                guard.connected = false;
                guard.tcp_writer = None;
                guard.broadcast_state();
                let resp = format!(r#"{{"ok":true,"soundbar":{}}}"#, guard.soundbar_state_json());
                send_json(&mut stream, &resp);
            }
        }
        ("POST", "/api/sj8/provision") => {
            let ssid = body_json
                .as_ref()
                .and_then(|b| b.get_str("ssid"))
                .unwrap_or("");
            let pwd = body_json
                .as_ref()
                .and_then(|b| b.get_str("password"))
                .unwrap_or("");
            let host = body_json
                .as_ref()
                .and_then(|b| b.get_str("setupHost"))
                .unwrap_or("192.168.5.100");
            let port = body_json
                .as_ref()
                .and_then(|b| b.get_i64("port"))
                .unwrap_or(9741) as u16;
            let cmd = format!(
                r#"{{"data":{{"home_ap_security_type":"WPA2","home_ap_ssid":{},"home_ap_ key":{},"bridge__enable":"no"}},"msg":"SHARE_HOME_INFO"}}"#,
                json_str(ssid),
                json_str(pwd)
            );
            match send_one_shot_lg_command(host, port, &cmd, EncryptionMode::Sj8V1) {
                Ok(replies) => {
                    let rep_str: Vec<String> = replies.into_iter().collect();
                    send_json(
                        &mut stream,
                        &format!(r#"{{"ok":true,"responses":[{}]}}"#, rep_str.join(",")),
                    );
                }
                Err(e) => send_json(
                    &mut stream,
                    &format!(r#"{{"ok":false,"error":{}}}"#, json_str(&e)),
                ),
            }
        }
        ("POST", "/api/sj8/register") => {
            let host = body_json
                .as_ref()
                .and_then(|b| b.get_str("host"))
                .unwrap_or("192.168.0.10");
            let port = body_json
                .as_ref()
                .and_then(|b| b.get_i64("port"))
                .unwrap_or(9741) as u16;
            let mac = body_json
                .as_ref()
                .and_then(|b| b.get_str("mac"))
                .unwrap_or("7c:1c:4e:3b:b0:22");
            let name = body_json
                .as_ref()
                .and_then(|b| b.get_str("name"))
                .unwrap_or("Sound Bar SJ8");
            let cmd = format!(
                r#"{{"data":{{"ip":{},"mac":{},"name":{}}},"msg":"SPK_ADD_SET"}}"#,
                json_str(host),
                json_str(mac),
                json_str(name)
            );
            match send_one_shot_lg_command(host, port, &cmd, EncryptionMode::Sj8V1) {
                Ok(replies) => send_json(
                    &mut stream,
                    &format!(r#"{{"ok":true,"responses":[{}]}}"#, replies.join(",")),
                ),
                Err(e) => send_json(
                    &mut stream,
                    &format!(r#"{{"ok":false,"error":{}}}"#, json_str(&e)),
                ),
            }
        }
        ("POST", "/api/volume") => {
            if let Ok(mut guard) = controller.lock() {
                let vol = body_json
                    .as_ref()
                    .and_then(|b| b.get_i64("volume"))
                    .unwrap_or(10)
                    .clamp(0, guard.max_volume);
                let fade = body_json
                    .as_ref()
                    .and_then(|b| b.get_i64("fadeSeconds"))
                    .unwrap_or(0);
                guard.volume = vol;
                guard.send_raw_json(
                    &format!(
                        r#"{{"msg":"VOLUME_SETTING","data":{{"volume":{},"fadetime":{}}}}}"#,
                        vol, fade
                    ),
                    None,
                );
                guard.broadcast_state();
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", "/api/mute") => {
            if let Ok(mut guard) = controller.lock() {
                let muted = body_json
                    .as_ref()
                    .and_then(|b| b.get_bool("muted"))
                    .unwrap_or(!guard.muted);
                guard.muted = muted;
                guard.send_raw_json(
                    &format!(r#"{{"msg":"MUTE_SET","data":{{"mute":{}}}}}"#, muted),
                    None,
                );
                guard.broadcast_state();
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", "/api/function") => {
            if let Ok(mut guard) = controller.lock() {
                let func = body_json
                    .as_ref()
                    .and_then(|b| b.get_i64("funcId"))
                    .unwrap_or(0);
                guard.current_function = func;
                guard.send_raw_json(
                    &format!(r#"{{"msg":"FUNCTION_SET","data":{{"func":{}}}}}"#, func),
                    None,
                );
                guard.broadcast_state();
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", "/api/eq") => {
            if let Ok(mut guard) = controller.lock() {
                let eq = body_json
                    .as_ref()
                    .and_then(|b| b.get_i64("eqId"))
                    .unwrap_or(0);
                guard.current_eq = eq;
                guard.send_raw_json(
                    &format!(r#"{{"msg":"EQ_SETTING","data":{{"eq":{}}}}}"#, eq),
                    None,
                );
                guard.broadcast_state();
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", "/api/tone") => {
            if let Ok(mut guard) = controller.lock() {
                if let Some(b) = body_json.as_ref() {
                    if let Some(v) = b.get_i64("bass") {
                        guard.bass = v;
                        guard.send_raw_json(
                            &format!(r#"{{"msg":"EQ_SETTING","data":{{"type":1,"value":{}}}}}"#, v),
                            None,
                        );
                    }
                    if let Some(v) = b.get_i64("treble") {
                        guard.treble = v;
                        guard.send_raw_json(
                            &format!(r#"{{"msg":"EQ_SETTING","data":{{"type":2,"value":{}}}}}"#, v),
                            None,
                        );
                    }
                    if let Some(v) = b.get_i64("balance") {
                        guard.balance = v;
                        guard.send_raw_json(
                            &format!(r#"{{"msg":"EQ_SETTING","data":{{"type":3,"value":{}}}}}"#, v),
                            None,
                        );
                    }
                    if b.get_bool("save").unwrap_or(false) {
                        guard.send_raw_json(
                            r#"{"msg":"EQ_SETTING","data":{"type":4,"value":1}}"#,
                            None,
                        );
                    }
                }
                guard.broadcast_state();
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", "/api/settings") => {
            if let Ok(mut guard) = controller.lock() {
                if let Some(b) = body_json.as_ref() {
                    if let Some(w) = b.get_i64("wooferLevel") {
                        guard.woofer_level = w.clamp(-15, 6);
                        guard.woofer_raw = guard.woofer_level - guard.woofer_offset;
                        let raw = guard.woofer_raw;
                        guard.send_raw_json(
                            &format!(
                                r#"{{"msg":"WOOFER_LEVEL_SET","data":{{"wooferlevel":{}}}}}"#,
                                raw
                            ),
                            None,
                        );
                    }
                    if let Some(v) = b.get_i64("avSyncMs") {
                        guard.av_sync_ms = v.clamp(0, 300);
                        guard.send_raw_json(
                            &format!(r#"{{"msg":"AV_SYNC_SET","data":{{"avsync":{}}}}}"#, v),
                            None,
                        );
                    }
                    if let Some(v) = b.get_i64("rearLevel") {
                        guard.rear_level = v.clamp(-6, 6);
                        guard.send_raw_json(
                            &format!(
                                r#"{{"msg":"REARBOX_LEVEL_SET","data":{{"rearboxlevel":{}}}}}"#,
                                v
                            ),
                            None,
                        );
                    }
                    if let Some(v) = b.get_i64("sleepTimerMinutes") {
                        guard.sleep_timer_minutes = v;
                        guard.send_raw_json(
                            &format!(r#"{{"msg":"SLEEP_SET","data":{{"time":{}}}}}"#, v),
                            None,
                        );
                    }
                    if let Some(flag) = b.get_bool("nightMode") {
                        guard.night_mode = flag;
                        guard.send_raw_json(
                            &format!(
                                r#"{{"msg":"NIGHT_MODE_SET","data":{{"nightmode":{}}}}}"#,
                                flag
                            ),
                            None,
                        );
                    }
                    if let Some(flag) = b.get_bool("autoVolume") {
                        guard.auto_volume = flag;
                        guard.send_raw_json(
                            &format!(r#"{{"msg":"AUTO_VOL_SET","data":{{"autovol":{}}}}}"#, flag),
                            None,
                        );
                    }
                    if let Some(flag) = b.get_bool("drc") {
                        guard.drc = flag;
                        guard.send_raw_json(
                            &format!(r#"{{"msg":"DRC_SET","data":{{"drc":{}}}}}"#, flag),
                            None,
                        );
                    }
                    if let Some(flag) = b.get_bool("autoPower") {
                        guard.auto_power = flag;
                        guard.send_raw_json(
                            &format!(
                                r#"{{"msg":"AUTO_POWER_SET","data":{{"autopower":{}}}}}"#,
                                flag
                            ),
                            None,
                        );
                    }
                    if let Some(flag) = b.get_bool("ledIndicator") {
                        guard.led_indicator = flag;
                        guard.send_raw_json(
                            &format!(r#"{{"msg":"LED_SET","data":{{"ledset":{}}}}}"#, flag),
                            None,
                        );
                    }
                    if let Some(flag) = b.get_bool("tvRemote") {
                        guard.tv_remote = flag;
                        guard.send_raw_json(
                            &format!(
                                r#"{{"msg":"TV_REMOTE_SET","data":{{"tvremote":{}}}}}"#,
                                flag
                            ),
                            None,
                        );
                    }
                    if let Some(flag) = b.get_bool("btStandby") {
                        guard.bt_standby = flag;
                        guard.send_raw_json(
                            &format!(
                                r#"{{"msg":"BT_STANDBY_SET","data":{{"btstandby":{}}}}}"#,
                                flag
                            ),
                            None,
                        );
                    }
                    if let Some(flag) = b.get_bool("btRestrict") {
                        guard.bt_restrict = flag;
                        guard.send_raw_json(
                            &format!(
                                r#"{{"msg":"BT_LIMIT_SET","data":{{"limit_bt_conn":{}}}}}"#,
                                flag
                            ),
                            None,
                        );
                    }
                }
                guard.broadcast_state();
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", "/api/test-tone") => {
            if let Ok(mut guard) = controller.lock() {
                guard.send_raw_json(r#"{"msg":"TEST_TONE","data":{"stat":true}}"#, None);
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", "/api/alarms") => {
            if let Ok(mut guard) = controller.lock() {
                if let Some(b) = body_json.as_ref() {
                    let days: Vec<u8> = b
                        .get_arr("daysList")
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| match v {
                                    crate::json::JsonValue::Number(n) => Some(*n as u8),
                                    _ => None,
                                })
                                .collect()
                        })
                        .unwrap_or_else(|| vec![0, 1, 2, 3, 4]);
                    let bitmask = days_list_to_bitmask(&days);
                    let assigned_id =
                        guard.alarms.iter().map(|a| a.id).max().unwrap_or(-1) + 1;
                    let source_type = b
                        .get_str("sourceType")
                        .unwrap_or("default_chime")
                        .to_string();
                    let entry = WifiAlarmEntry {
                        id: assigned_id,
                        title: b
                            .get_str("title")
                            .unwrap_or("SJ8 Wakeup Alarm")
                            .to_string(),
                        hour: b.get_i64("hour").unwrap_or(7).clamp(0, 23),
                        minute: b.get_i64("minute").unwrap_or(30).clamp(0, 59),
                        days_bitmask: bitmask,
                        days_list: days,
                        day_repeat: true,
                        volume: b.get_i64("volume").unwrap_or(15).clamp(1, 40),
                        duration_minutes: b.get_i64("durationMinutes").unwrap_or(30),
                        enable: true,
                        shuffle: false,
                        source_type: source_type.clone(),
                        spotify_uri: b
                            .get_str("spotifyUri")
                            .unwrap_or("spotify:playlist:37i9dQZF1DXbITWG1ZJKYt")
                            .to_string(),
                        local_track_id: b
                            .get_str("localTrackId")
                            .unwrap_or("track-sunrise-flac")
                            .to_string(),
                        target_input: 0,
                        fade_in_seconds: 15,
                        synced_to_hardware: true,
                    };
                    let alarm_type = if source_type == "default_chime" { 0 } else { 1 };
                    let songpath = if source_type == "default_chime" {
                        "".to_string()
                    } else if source_type == "spotify" {
                        entry.spotify_uri.clone()
                    } else {
                        format!(
                            "http://127.0.0.1:{}/stream/{}.wav",
                            LG_DMS_HTTP_PORT, entry.local_track_id
                        )
                    };
                    let cmd = format!(
                        r#"{{"msg":"ALARM_SET","data":{{"id":-1,"modifiedid":{},"mode":0,"day":{},"dayrepeat":true,"hour":{},"minute":{},"type":{},"duration":{},"volume":{},"enable":true,"shuffle":false,"title":{},"ipaddress":"","songpath":{}}}}}"#,
                        entry.id,
                        entry.days_bitmask,
                        entry.hour,
                        entry.minute,
                        alarm_type,
                        entry.duration_minutes,
                        entry.volume,
                        json_str(&entry.title),
                        json_str(&songpath)
                    );
                    guard.alarms.push(entry);
                    guard.send_raw_json(&cmd, None);
                    guard.broadcast_state();
                }
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", p) if p.starts_with("/api/alarms/") && p.ends_with("/toggle") => {
            let id_str = p
                .trim_start_matches("/api/alarms/")
                .trim_end_matches("/toggle");
            let alarm_id: i64 = id_str.parse().unwrap_or(-1);
            let enable = body_json
                .as_ref()
                .and_then(|b| b.get_bool("enable"))
                .unwrap_or(true);
            if let Ok(mut guard) = controller.lock() {
                if let Some(a) = guard.alarms.iter_mut().find(|a| a.id == alarm_id) {
                    a.enable = enable;
                }
                let mode = if enable { 3 } else { 4 };
                guard.send_raw_json(
                    &format!(
                        r#"{{"msg":"ALARM_SET","data":{{"id":{},"mode":{},"enable":{}}}}}"#,
                        alarm_id, mode, enable
                    ),
                    None,
                );
                guard.broadcast_state();
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", p) if p.starts_with("/api/alarms/") && p.ends_with("/trigger") => {
            let id_str = p
                .trim_start_matches("/api/alarms/")
                .trim_end_matches("/trigger");
            let alarm_id: i64 = id_str.parse().unwrap_or(-1);
            if let Ok(mut guard) = controller.lock() {
                if let Some(a) = guard.alarms.iter().find(|a| a.id == alarm_id).cloned() {
                    guard.volume = a.volume;
                    guard.send_raw_json(
                        &format!(
                            r#"{{"msg":"VOLUME_SETTING","data":{{"volume":{},"fadetime":5}}}}"#,
                            a.volume
                        ),
                        None,
                    );
                    guard.broadcast_state();
                }
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("DELETE", p) if p.starts_with("/api/alarms/") => {
            let id_str = p.trim_start_matches("/api/alarms/");
            let alarm_id: i64 = id_str.parse().unwrap_or(-1);
            if let Ok(mut guard) = controller.lock() {
                guard.alarms.retain(|a| a.id != alarm_id);
                guard.send_raw_json(
                    &format!(
                        r#"{{"msg":"ALARM_SET","data":{{"id":{},"mode":2}}}}"#,
                        alarm_id
                    ),
                    None,
                );
                guard.broadcast_state();
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", "/api/spotify/config") => {
            if let Ok(mut guard) = controller.lock() {
                if let Some(cid) = body_json.as_ref().and_then(|b| b.get_str("clientId")) {
                    guard.spotify_client_id = cid.to_string();
                }
                let sp = guard.spotify_state_json();
                send_json(&mut stream, &format!(r#"{{"ok":true,"spotify":{}}}"#, sp));
            }
        }
        ("POST", "/api/spotify/transfer") => {
            if let Ok(mut guard) = controller.lock() {
                guard.current_function = 8;
                guard.send_raw_json(r#"{"msg":"FUNCTION_SET","data":{"func":8}}"#, None);
                guard.broadcast_state();
                let sp = guard.spotify_state_json();
                send_json(&mut stream, &format!(r#"{{"ok":true,"spotify":{}}}"#, sp));
            }
        }
        ("POST", "/api/spotify/play") => {
            if let Ok(mut guard) = controller.lock() {
                let uri = body_json
                    .as_ref()
                    .and_then(|b| b.get_str("uri"))
                    .unwrap_or("spotify:playlist:37i9dQZF1DXbITWG1ZJKYt");
                guard.current_function = 8;
                guard.spotify_playing = true;
                guard.play_playing = true;
                guard.send_raw_json(
                    &format!(
                        r#"{{"msg":"CP_PLAY_URL","data":{{"cp":"spotify","uri":{}}}}}"#,
                        json_str(uri)
                    ),
                    None,
                );
                guard.broadcast_state();
                let sp = guard.spotify_state_json();
                send_json(&mut stream, &format!(r#"{{"ok":true,"spotify":{}}}"#, sp));
            }
        }
        ("POST", "/api/spotify/toggle") => {
            if let Ok(mut guard) = controller.lock() {
                guard.spotify_playing = !guard.spotify_playing;
                guard.play_playing = guard.spotify_playing;
                guard.broadcast_state();
                let sp = guard.spotify_state_json();
                send_json(&mut stream, &format!(r#"{{"ok":true,"spotify":{}}}"#, sp));
            }
        }
        ("POST", p) if p.starts_with("/api/media/") && p.ends_with("/beam") => {
            let track_id = p
                .trim_start_matches("/api/media/")
                .trim_end_matches("/beam");
            let url = format!("http://127.0.0.1:{}/stream/{}.wav", api_port, track_id);
            if let Ok(mut guard) = controller.lock() {
                guard.current_function = 0;
                guard.play_title = track_id.to_string();
                guard.play_source_app = "MusicFlow DMS (:49252)".to_string();
                guard.send_raw_json(
                    &format!(
                        r#"{{"msg":"LOCAL_PLAY_URL","data":{{"url":{},"title":{}}}}}"#,
                        json_str(&url),
                        json_str(track_id)
                    ),
                    None,
                );
                guard.broadcast_state();
            }
            send_json(&mut stream, &format!(r#"{{"ok":true,"streamUrl":{}}}"#, json_str(&url)));
        }
        ("POST", p) if p.starts_with("/api/mesh/") => {
            let node_id = p.trim_start_matches("/api/mesh/");
            if let Ok(mut guard) = controller.lock() {
                if let Some(ch) = body_json
                    .as_ref()
                    .and_then(|b| b.get_str("channelAssignment"))
                {
                    if let Some(node) = guard.mesh_nodes.iter_mut().find(|n| n.id == node_id) {
                        node.channel_assignment = ch.to_string();
                    }
                    guard.send_raw_json(
                        &format!(
                            r#"{{"msg":"SPK_CH_SET","data":{{"id":{},"channel":{}}}}}"#,
                            json_str(node_id),
                            json_str(ch)
                        ),
                        None,
                    );
                }
                guard.broadcast_state();
                let state = guard.soundbar_state_json();
                send_json(&mut stream, &format!(r#"{{"ok":true,"soundbar":{}}}"#, state));
            }
        }
        ("POST", "/api/raw-packet") => {
            if let Ok(mut guard) = controller.lock() {
                if let Some(b) = body_json.as_ref() {
                    if let Some(payload) = b.get("payload") {
                        let override_mode = b
                            .get_str("encryption")
                            .map(EncryptionMode::from_str);
                        guard.send_raw_json(&payload.to_json_string(), override_mode);
                    }
                }
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        ("POST", "/api/logs/clear") => {
            if let Ok(mut guard) = controller.lock() {
                guard.packet_logs.clear();
                guard.broadcast_state();
            }
            send_json(&mut stream, r#"{"ok":true}"#);
        }
        _ => {
            send_response(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX_HTML);
        }
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn send_json(stream: &mut TcpStream, json_body: &str) {
    send_response(
        stream,
        "200 OK",
        "application/json; charset=utf-8",
        json_body.as_bytes(),
    );
}

fn send_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_lg_sj8_controller_and_simulator() {
        let port = start_mock_soundbar_server(29841);
        let ctrl = Arc::new(Mutex::new(SoundbarController::new()));
        let ok = connect_soundbar_tcp(&ctrl, "127.0.0.1", port, EncryptionMode::Sj8V1, true);
        assert!(ok);

        thread::sleep(Duration::from_millis(120));
        {
            let mut guard = ctrl.lock().unwrap();
            assert!(guard.connected);
            assert_eq!(guard.model_name, "SJ8");
            assert_eq!(guard.pet_name, "Sound Bar SJ8");
            assert_eq!(guard.firmware_version, "NB9.504.00629.C");
            assert_eq!(guard.woofer_offset, -15);
            assert_eq!(guard.woofer_raw, 15);
            assert_eq!(guard.woofer_level, 0);

            guard.send_raw_json(r#"{"msg":"VOLUME_SETTING","data":{"volume":24,"fadetime":0}}"#, None);
            guard.send_raw_json(r#"{"msg":"EQ_SETTING","data":{"eq":14}}"#, None);
            guard.send_raw_json(r#"{"msg":"WOOFER_LEVEL_SET","data":{"wooferlevel":19}}"#, None);
        }

        thread::sleep(Duration::from_millis(100));
        {
            let guard = ctrl.lock().unwrap();
            assert_eq!(guard.volume, 24);
            assert_eq!(guard.current_eq, 14);
            assert_eq!(guard.woofer_raw, 19);
            assert_eq!(guard.woofer_level, 4);
        }
    }
}

