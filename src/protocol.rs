//! Pure-Rust LG SJ8 TCP 9741 Client, State Engine, Spotify Connect Bridge,
//! Media Library Synthesizer, and SSDP/Wi-Fi Provisioning Manager.

use crate::crypto::{
    bitmask_to_days_list, encode_lg_packet, EncryptionMode, LgPacketFramer,
};
use crate::json::{json_str, parse_json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const LG_DEFAULT_TCP_PORT: u16 = 9741;
pub const LG_DMS_HTTP_PORT: u16 = 49252;

#[derive(Clone)]
pub struct WifiAlarmEntry {
    pub id: i64,
    pub title: String,
    pub hour: i64,
    pub minute: i64,
    pub days_bitmask: u8,
    pub days_list: Vec<u8>,
    pub day_repeat: bool,
    pub volume: i64,
    pub duration_minutes: i64,
    pub enable: bool,
    pub shuffle: bool,
    pub source_type: String,
    pub spotify_uri: String,
    pub local_track_id: String,
    pub target_input: i64,
    pub fade_in_seconds: i64,
    pub synced_to_hardware: bool,
}

#[derive(Clone)]
pub struct PacketLogEntry {
    pub id: String,
    pub timestamp: String,
    pub direction: String,
    pub encrypted: bool,
    pub cipher_profile: String,
    pub raw_byte_length: usize,
    pub msg_type: String,
    pub payload_json: String,
}

#[derive(Clone)]
pub struct MeshNode {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub model: String,
    pub role: String,
    pub channel_assignment: String,
    pub group_id: i64,
    pub volume: i64,
    pub muted: bool,
    pub online: bool,
    pub mesh_rssi: i64,
}

pub struct SoundbarController {
    pub connected: bool,
    pub host: String,
    pub port: u16,
    pub dialect: String,
    pub encryption: EncryptionMode,
    pub is_simulator: bool,

    pub name: String,
    pub model_name: String,
    pub pet_name: String,
    pub firmware_version: String,
    pub micom_version: String,
    pub hdmi_version: String,
    pub c4a_version: String,
    pub mac_address: String,
    pub bt_mac_address: String,
    pub lan_mac_address: String,
    pub ssid: String,
    pub rssi: i64,
    pub radio_part_status: i64,

    pub current_function: i64,
    pub cp_type: i64,
    pub volume: i64,
    pub max_volume: i64,
    pub muted: bool,
    pub current_eq: i64,
    pub bass: i64,
    pub treble: i64,
    pub balance: i64,

    pub night_mode: bool,
    pub auto_volume: bool,
    pub drc: bool,
    pub neural_x: bool,
    pub av_sync_ms: i64,
    pub woofer_level: i64,  // -15 .. +6 dB
    pub woofer_raw: i64,    // 0 .. 21
    pub woofer_offset: i64, // -15
    pub rear_enabled: bool,
    pub rear_level: i64,
    pub center_level: i64,
    pub top_level: i64,
    pub tv_remote: bool,
    pub auto_power: bool,
    pub auto_display: bool,
    pub bt_standby: bool,
    pub bt_restrict: bool,
    pub led_indicator: bool,
    pub sleep_timer_minutes: i64,

    pub play_title: String,
    pub play_artist: String,
    pub play_album: String,
    pub play_source_app: String,
    pub play_playing: bool,
    pub play_pos_sec: i64,
    pub play_dur_sec: i64,
    pub play_repeat: i64,
    pub play_shuffle: bool,
    pub play_codec: String,

    pub alarms: Vec<WifiAlarmEntry>,
    pub mesh_nodes: Vec<MeshNode>,
    pub packet_logs: Vec<PacketLogEntry>,

    pub spotify_client_id: String,
    pub spotify_playing: bool,

    pub tcp_writer: Option<TcpStream>,
    pub connection_gen: u64,
    ws_clients: Vec<TcpStream>,
    pkt_counter: u64,
}

impl SoundbarController {
    pub fn new() -> Self {
        Self {
            connected: false,
            host: "192.168.0.10".to_string(),
            port: LG_DEFAULT_TCP_PORT,
            dialect: "auto".to_string(),
            encryption: EncryptionMode::Sj8V1,
            is_simulator: true,

            name: "LG SJ8".to_string(),
            model_name: "SJ8".to_string(),
            pet_name: "Sound Bar SJ8".to_string(),
            firmware_version: "NB9.504.00629.C".to_string(),
            micom_version: "1904080".to_string(),
            hdmi_version: "1807100".to_string(),
            c4a_version: "1.36.145853".to_string(),
            mac_address: "7c:1c:4e:3b:b0:22".to_string(),
            bt_mac_address: "08:EF:3B:9C:2D:4F".to_string(),
            lan_mac_address: "00:e0:4c:36:14:98".to_string(),
            ssid: "Adnan_Home_WiFi".to_string(),
            rssi: -46,
            radio_part_status: 1,

            current_function: 8,
            cp_type: 2,
            volume: 10,
            max_volume: 40,
            muted: false,
            current_eq: 12,
            bass: 0,
            treble: 0,
            balance: 0,

            night_mode: false,
            auto_volume: false,
            drc: true,
            neural_x: true,
            av_sync_ms: 0,
            woofer_level: 0,
            woofer_raw: 15,
            woofer_offset: -15,
            rear_enabled: false,
            rear_level: 0,
            center_level: 0,
            top_level: 0,
            tv_remote: true,
            auto_power: true,
            auto_display: true,
            bt_standby: true,
            bt_restrict: false,
            led_indicator: true,
            sleep_timer_minutes: 0,

            play_title: "Bandit".to_string(),
            play_artist: "EsDeeKid".to_string(),
            play_album: "Bandit - Single".to_string(),
            play_source_app: "Spotify".to_string(),
            play_playing: true,
            play_pos_sec: 49,
            play_dur_sec: 136,
            play_repeat: 0,
            play_shuffle: true,
            play_codec: "Spotify Ogg Vorbis 320kbps / 44.1kHz".to_string(),

            alarms: vec![
                WifiAlarmEntry {
                    id: 0,
                    title: "Weekday SJ8 Built-in Chime".to_string(),
                    hour: 7,
                    minute: 30,
                    days_bitmask: 124,
                    days_list: vec![0, 1, 2, 3, 4],
                    day_repeat: true,
                    volume: 12,
                    duration_minutes: 30,
                    enable: true,
                    shuffle: false,
                    source_type: "default_chime".to_string(),
                    spotify_uri: "spotify:playlist:37i9dQZF1DXbITWG1ZJKYt".to_string(),
                    local_track_id: "track-sunrise-flac".to_string(),
                    target_input: 0,
                    fade_in_seconds: 15,
                    synced_to_hardware: true,
                },
                WifiAlarmEntry {
                    id: 1,
                    title: "Weekend Local FLAC Wakeup".to_string(),
                    hour: 9,
                    minute: 0,
                    days_bitmask: 3,
                    days_list: vec![5, 6],
                    day_repeat: true,
                    volume: 15,
                    duration_minutes: 45,
                    enable: false,
                    shuffle: true,
                    source_type: "local_stream".to_string(),
                    spotify_uri: "spotify:playlist:37i9dQZF1DX4sWSpwq3LiO".to_string(),
                    local_track_id: "track-sunrise-flac".to_string(),
                    target_input: 0,
                    fade_in_seconds: 20,
                    synced_to_hardware: true,
                },
            ],
            mesh_nodes: vec![
                MeshNode {
                    id: "spk-sj8-main".to_string(),
                    name: "Sound Bar SJ8 (Master)".to_string(),
                    ip: "192.168.0.10".to_string(),
                    model: "LG SJ8 (4.1ch 300W)".to_string(),
                    role: "surround_master".to_string(),
                    channel_assignment: "stereo".to_string(),
                    group_id: 1,
                    volume: 10,
                    muted: false,
                    online: true,
                    mesh_rssi: -46,
                },
                MeshNode {
                    id: "spk-h4-rear-l".to_string(),
                    name: "Rear Left (NP8350 / H4)".to_string(),
                    ip: "192.168.0.11".to_string(),
                    model: "LG H4 (NP8350)".to_string(),
                    role: "surround_slave".to_string(),
                    channel_assignment: "rear_left".to_string(),
                    group_id: 1,
                    volume: 10,
                    muted: false,
                    online: true,
                    mesh_rssi: -52,
                },
                MeshNode {
                    id: "spk-h4-rear-r".to_string(),
                    name: "Rear Right (NP8350 / H4)".to_string(),
                    ip: "192.168.0.12".to_string(),
                    model: "LG H4 (NP8350)".to_string(),
                    role: "surround_slave".to_string(),
                    channel_assignment: "rear_right".to_string(),
                    group_id: 1,
                    volume: 10,
                    muted: false,
                    online: true,
                    mesh_rssi: -54,
                },
            ],
            packet_logs: Vec::new(),
            spotify_client_id: "lg-musicflow-open-pkce-client".to_string(),
            spotify_playing: true,
            tcp_writer: None,
            connection_gen: 0,
            ws_clients: Vec::new(),
            pkt_counter: 0,
        }
    }

    pub fn add_ws_client(&mut self, stream: TcpStream) {
        self.ws_clients.push(stream);
    }

    pub fn broadcast_ws(&mut self, msg_type: &str, data_json: &str) {
        let payload = format!(r#"{{"type":{},"data":{}}}"#, json_str(msg_type), data_json);
        let frame = encode_ws_text_frame(&payload);
        self.ws_clients.retain_mut(|client| client.write_all(&frame).is_ok());
    }

    pub fn broadcast_state(&mut self) {
        let state_json = self.soundbar_state_json();
        self.broadcast_ws("state", &state_json);
    }

    pub fn alarms_array_json(&self) -> String {
        let alarms_str: Vec<String> = self
            .alarms
            .iter()
            .map(|a| {
                let days_str: Vec<String> = a.days_list.iter().map(|d| d.to_string()).collect();
                format!(
                    r#"{{"id":{},"title":{},"hour":{},"minute":{},"daysBitmask":{},"daysList":[{}],"dayRepeat":{},"volume":{},"durationMinutes":{},"enable":{},"shuffle":{},"sourceType":{},"spotifyUri":{},"localTrackId":{},"targetInput":{},"fadeInSeconds":{},"syncedToHardware":{}}}"#,
                    a.id,
                    json_str(&a.title),
                    a.hour,
                    a.minute,
                    a.days_bitmask,
                    days_str.join(","),
                    a.day_repeat,
                    a.volume,
                    a.duration_minutes,
                    a.enable,
                    a.shuffle,
                    json_str(&a.source_type),
                    json_str(&a.spotify_uri),
                    json_str(&a.local_track_id),
                    a.target_input,
                    a.fade_in_seconds,
                    a.synced_to_hardware
                )
            })
            .collect();
        format!("[{}]", alarms_str.join(","))
    }

    pub fn mesh_nodes_array_json(&self) -> String {
        let mesh_str: Vec<String> = self
            .mesh_nodes
            .iter()
            .map(|m| {
                format!(
                    r#"{{"id":{},"name":{},"ip":{},"model":{},"role":{},"channelAssignment":{},"groupId":{},"volume":{},"muted":{},"online":{},"meshRssi":{}}}"#,
                    json_str(&m.id),
                    json_str(&m.name),
                    json_str(&m.ip),
                    json_str(&m.model),
                    json_str(&m.role),
                    json_str(&m.channel_assignment),
                    m.group_id,
                    m.volume,
                    m.muted,
                    m.online,
                    m.mesh_rssi
                )
            })
            .collect();
        format!("[{}]", mesh_str.join(","))
    }

    pub fn packet_logs_array_json(&self) -> String {
        let logs_str: Vec<String> = self
            .packet_logs
            .iter()
            .take(50)
            .map(|l| {
                format!(
                    r#"{{"id":{},"timestamp":{},"direction":{},"encrypted":{},"cipherProfile":{},"headerHex":"10","rawByteLength":{},"msgType":{},"payload":{}}}"#,
                    json_str(&l.id),
                    json_str(&l.timestamp),
                    json_str(&l.direction),
                    l.encrypted,
                    json_str(&l.cipher_profile),
                    l.raw_byte_length,
                    json_str(&l.msg_type),
                    l.payload_json
                )
            })
            .collect();
        format!("[{}]", logs_str.join(","))
    }

    fn record_packet(
        &mut self,
        direction: &str,
        encrypted: bool,
        cipher: EncryptionMode,
        raw_len: usize,
        msg_type: &str,
        payload_json: &str,
    ) {
        self.pkt_counter += 1;
        let ts = iso_timestamp_now();
        let entry = PacketLogEntry {
            id: format!("pkt-{}", self.pkt_counter),
            timestamp: ts.clone(),
            direction: direction.to_string(),
            encrypted,
            cipher_profile: cipher.as_str().to_string(),
            raw_byte_length: raw_len,
            msg_type: msg_type.to_string(),
            payload_json: payload_json.to_string(),
        };
        let header_hex = if encrypted { "10" } else { "00" };
        let entry_json = format!(
            r#"{{"id":{},"timestamp":{},"direction":{},"encrypted":{},"cipherProfile":{},"headerHex":{},"rawByteLength":{},"msgType":{},"payload":{}}}"#,
            json_str(&entry.id),
            json_str(&entry.timestamp),
            json_str(&entry.direction),
            entry.encrypted,
            json_str(&entry.cipher_profile),
            json_str(header_hex),
            entry.raw_byte_length,
            json_str(&entry.msg_type),
            entry.payload_json
        );
        self.packet_logs.insert(0, entry);
        if self.packet_logs.len() > 80 {
            self.packet_logs.truncate(80);
        }
        crate::logger::log_debug(
            "TCP-9741",
            &format!(
                "{} {} ({}B, {}) {}",
                direction.to_uppercase(),
                msg_type,
                raw_len,
                cipher.as_str(),
                payload_json
            ),
        );
        self.broadcast_ws("packet", &entry_json);
    }

    pub fn send_raw_json(&mut self, json_payload: &str, override_mode: Option<EncryptionMode>) {
        let mode = override_mode.unwrap_or(self.encryption);
        let pkt = encode_lg_packet(json_payload, mode);
        if let Some(writer) = &mut self.tcp_writer {
            let _ = writer.write_all(&pkt);
        }
        let msg_type = parse_json(json_payload)
            .and_then(|v| v.get_str("msg").map(|s| s.to_string()))
            .unwrap_or_else(|| "RAW_CMD".to_string());
        self.record_packet(
            "tx",
            mode != EncryptionMode::Plain,
            mode,
            pkt.len(),
            &msg_type,
            json_payload,
        );
    }

    pub fn send_play_cmd(
        &mut self,
        playctrl: i64,
        repeat_override: Option<i64>,
        shuffle_override: Option<bool>,
    ) {
        if playctrl == 0 {
            self.play_playing = true;
            self.spotify_playing = true;
        } else if playctrl == 1 || playctrl == 2 {
            self.play_playing = false;
            self.spotify_playing = false;
        }

        let repeat = match repeat_override {
            Some(r) => r.clamp(0, 2),
            None if playctrl == 7 => (self.play_repeat + 1) % 3,
            None => self.play_repeat,
        };
        let shuffle = match shuffle_override {
            Some(s) => s,
            None if playctrl == 8 => !self.play_shuffle,
            None => self.play_shuffle,
        };

        if playctrl == 7 || repeat_override.is_some() {
            self.play_repeat = repeat;
        }
        if playctrl == 8 || shuffle_override.is_some() {
            self.play_shuffle = shuffle;
        }

        let payload = format!(
            r#"{{"msg":"PLAY_CMD","data":{{"playctrl":{},"repeat":{},"shuffle":{}}}}}"#,
            playctrl, repeat, shuffle
        );
        self.send_raw_json(&payload, None);
    }

    pub fn send_seek_cmd(&mut self, time_sec: i64) {
        let clamped = time_sec.max(0);
        self.play_pos_sec = clamped;
        let payload = format!(
            r#"{{"msg":"LOCAL_TIME_SEARCH","data":{{"time":{}}}}}"#,
            clamped
        );
        self.send_raw_json(&payload, None);
    }

    pub fn query_initial_sj8_state(&mut self) {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let days_since_epoch = secs / 86400;
        // 1970-01-01 was Thursday (index 3 when Mon=0..Sun=6)
        let lg_day = (days_since_epoch + 3) % 7;
        let hour = (secs / 3600) % 24;
        let min = (secs / 60) % 60;
        let prod_cmd = format!(
            r#"{{"msg":"PRODUCT_INFO","data":{{"day":{},"hour":{},"id":"mac7085C23CCA2A","min":{},"option":0}}}}"#,
            lg_day, hour, min
        );
        let cmds: [&str; 13] = [
            &prod_cmd,
            r#"{"msg":"SYSTEM_VER_REQ"}"#,
            r#"{"msg":"PLAY_INFO_REQ"}"#,
            r#"{"msg":"PLAY_TIME_SET","data":{"set":true}}"#,
            r#"{"msg":"FUNC_INFO_REQ"}"#,
            r#"{"msg":"ALARM_STATE_REQ"}"#,
            r#"{"msg":"SETTING_INFO_REQ"}"#,
            r#"{"msg":"FUNC_NAME_REQ","data":{"func":15}}"#,
            r#"{"msg":"EQ_INFO_REQ"}"#,
            r#"{"msg":"ALARM_LIST_REQ"}"#,
            r#"{"msg":"SLEEP_INFO_REQ"}"#,
            r#"{"msg":"NETWORK_INFO_REQ"}"#,
            r#"{"msg":"CP_INFO_REQ"}"#,
        ];
        for cmd in cmds {
            self.send_raw_json(cmd, None);
        }
    }

    pub fn handle_incoming_frame(
        &mut self,
        encrypted: bool,
        cipher: EncryptionMode,
        raw_len: usize,
        json_text: &str,
    ) {
        let parsed = match parse_json(json_text) {
            Some(v) => v,
            None => return,
        };
        let msg = parsed.get_str("msg").unwrap_or("UNKNOWN").to_string();
        self.record_packet("rx", encrypted, cipher, raw_len, &msg, json_text);

        if let Some(data) = parsed.get("data") {
            match msg.as_str() {
                "PRODUCT_INFO" => {
                    if let Some(s) = data.get_str("modelname") {
                        self.model_name = s.to_string();
                    }
                    if let Some(s) = data.get_str("petname") {
                        self.pet_name = s.to_string();
                    }
                    if let Some(info) = data.get("info") {
                        if let Some(s) = info.get_str("name") {
                            self.name = s.to_string();
                        }
                        if let Some(s) = info.get_str("modelname") {
                            self.model_name = s.to_string();
                        }
                        if let Some(s) = info.get_str("petname") {
                            self.pet_name = s.to_string();
                        }
                        if let Some(s) = info.get_str("wirelessmac").or_else(|| info.get_str("mac")) {
                            self.mac_address = s.to_string();
                        }
                        if let Some(s) = info.get_str("btmac") {
                            self.bt_mac_address = s.to_string();
                        }
                        if let Some(s) = info.get_str("bever") {
                            self.firmware_version = s.to_string();
                        }
                        if let Some(v) = info.get_i64("vol") {
                            if v <= self.max_volume {
                                self.volume = v;
                            }
                        }
                        if let Some(m) = info.get_bool("mute") {
                            self.muted = m;
                        }
                        if let Some(f) = info.get_i64("function") {
                            self.current_function = f;
                        }
                        if let Some(p) = info.get_bool("playing") {
                            self.play_playing = p;
                            self.spotify_playing = p;
                        }
                        if let Some(l) = info.get_bool("ledset") {
                            self.led_indicator = l;
                        }
                    }
                }
                "SYSTEM_VER_REQ" | "SYSTEM_VER_INFO" => {
                    if let Some(s) = data.get_str("be").or_else(|| data.get_str("system")) {
                        self.firmware_version = s.to_string();
                    }
                    if let Some(s) = data.get_str("micom") {
                        self.micom_version = s.to_string();
                    }
                    if let Some(s) = data.get_str("hdmi") {
                        self.hdmi_version = s.to_string();
                    }
                    if let Some(s) = data.get_str("c4a") {
                        self.c4a_version = s.to_string();
                    }
                }
                "MAC_INFO_DEV" => {
                    if let Some(s) = data.get_str("btmac") {
                        self.bt_mac_address = s.to_string();
                    }
                    if let Some(s) = data.get_str("lanmac") {
                        self.lan_mac_address = s.to_string();
                    }
                    if let Some(s) = data.get_str("wifimac") {
                        self.mac_address = s.to_string();
                    }
                }
                "NETWORK_INFO_REQ" | "NET_INFO_DEV" | "SHARE_AP_INFO" => {
                    if let Some(s) = data.get_str("ssid").or_else(|| data.get_str("s_ssid")) {
                        self.ssid = s.to_string();
                    }
                }
                "SPK_NAME_CHANGE" => {
                    if let Some(s) = data.get_str("name") {
                        if !s.is_empty() {
                            self.name = s.to_string();
                        }
                    }
                }
                "FUNC_INFO" | "FUNC_INFO_REQ" => {
                    if let Some(f) = data.get_i64("type") {
                        self.current_function = f;
                    }
                    if let Some(cp) = data.get_i64("cptype") {
                        self.cp_type = cp;
                    }
                    if let Some(m) = data.get_bool("mute") {
                        self.muted = m;
                    }
                }
                "EQ_INFO" | "EQ_INFO_REQ" | "EQ_NOTI_CHG" => {
                    if let Some(eq) = data.get_i64("currenteq").or_else(|| data.get_i64("eq")) {
                        self.current_eq = eq;
                    }
                    if let Some(b) = data.get_i64("bass") {
                        self.bass = b;
                    }
                    if let Some(t) = data.get_i64("treble") {
                        self.treble = t;
                    }
                    if let Some(bal) = data.get_i64("lrbal").or_else(|| data.get_i64("balance")) {
                        self.balance = bal;
                    }
                }
                "VOLUME_CHANGE" | "VOLUME_INFO" | "VOLUME_SETTING" => {
                    if let Some(v) = data.get_i64("vol").or_else(|| data.get_i64("volume")) {
                        self.volume = v;
                    }
                    if let Some(m) = data.get_bool("mute") {
                        self.muted = m;
                    }
                }
                "MUTE_CHANGE" | "MUTE_SET" => {
                    if let Some(m) = data.get_bool("mute") {
                        self.muted = m;
                    }
                }
                "PLAY_INFO" | "PLAY_INFO_REQ" | "CP_INFO_REQ" => {
                    if let Some(v) = data.get_i64("vol").or_else(|| data.get_i64("volume")) {
                        self.volume = v;
                    }
                    if let Some(m) = data.get_bool("mute") {
                        self.muted = m;
                    }
                    if let Some(t) = data.get_str("title") {
                        if !t.is_empty() {
                            self.play_title = t.to_string();
                        }
                    }
                    if let Some(a) = data.get_str("artist") {
                        if !a.is_empty() {
                            self.play_artist = a.to_string();
                        }
                    }
                    if let Some(al) = data.get_str("albumtitle").or_else(|| data.get_str("album")) {
                        if !al.is_empty() {
                            self.play_album = al.to_string();
                        }
                    }
                    if let Some(cp) = data.get_str("c4aappname").or_else(|| data.get_str("cpname")) {
                        if !cp.is_empty() {
                            self.play_source_app = cp.to_string();
                        }
                    }
                    if let Some(pos) = data.get_i64("position") {
                        self.play_pos_sec = pos;
                    }
                    if let Some(dur) = data.get_i64("duration") {
                        self.play_dur_sec = dur;
                    }
                    if let Some(p) = data.get_i64("playing") {
                        // APK PlayInfoResponse.isPlaying(): 0=NORMAL, 4=BUFFERING
                        let is_playing = p == 0 || p == 4;
                        self.play_playing = is_playing;
                        self.spotify_playing = is_playing;
                    }
                    if let Some(rep) = data.get_i64("repeat") {
                        self.play_repeat = rep;
                    }
                    if let Some(shuf) = data.get_bool("shuffle") {
                        self.play_shuffle = shuf;
                    }
                    if let Some(cp) = data.get_i64("cptype") {
                        self.cp_type = cp;
                    }
                }
                "PLAY_TIME" => {
                    if let Some(pos) = data.get_i64("position") {
                        self.play_pos_sec = pos;
                    }
                    if let Some(dur) = data.get_i64("duration") {
                        self.play_dur_sec = dur;
                    }
                    if let Some(p) = data.get_i64("playing") {
                        let is_playing = p == 0 || p == 4;
                        self.play_playing = is_playing;
                        self.spotify_playing = is_playing;
                    }
                }
                "PLAY_CMD" => {
                    if let Some(ctrl) = data.get_i64("playctrl") {
                        if ctrl == 0 {
                            self.play_playing = true;
                            self.spotify_playing = true;
                        } else if ctrl == 1 || ctrl == 2 {
                            self.play_playing = false;
                            self.spotify_playing = false;
                        }
                    }
                    if let Some(rep) = data.get_i64("repeat") {
                        self.play_repeat = rep;
                    }
                    if let Some(shuf) = data.get_bool("shuffle") {
                        self.play_shuffle = shuf;
                    }
                }
                "LOCAL_TIME_SEARCH" => {
                    if let Some(t) = data.get_i64("time") {
                        self.play_pos_sec = t;
                    }
                }
                "SETTING_INFO" | "SETTING_INFO_REQ" | "SETTING_INFO_NOTI" => {
                    if let Some(off) = data.get_i64("wooferoffset") {
                        self.woofer_offset = off;
                    }
                    if let Some(w) = data.get_i64("wooferlevel") {
                        self.woofer_raw = w;
                        self.woofer_level = w + self.woofer_offset;
                    }
                    if let Some(b) = data.get_bool("nightmode") {
                        self.night_mode = b;
                    }
                    if let Some(b) = data.get_bool("autovol") {
                        self.auto_volume = b;
                    }
                    if let Some(b) = data.get_bool("drc") {
                        self.drc = b;
                    }
                    if let Some(v) = data.get_i64("avsync") {
                        self.av_sync_ms = v;
                    }
                    if let Some(v) = data.get_i64("rearboxlevel") {
                        let rear_off = data.get_i64("rearboxoffset").unwrap_or(0);
                        self.rear_level = v + rear_off;
                    }
                    if let Some(b) = data.get_bool("rearboxon") {
                        self.rear_enabled = b;
                    }
                    if let Some(b) = data.get_bool("autopower") {
                        self.auto_power = b;
                    }
                    if let Some(b) = data.get_bool("autodisplay") {
                        self.auto_display = b;
                    }
                    if let Some(b) = data.get_bool("ledset") {
                        self.led_indicator = b;
                    }
                    if let Some(b) = data.get_bool("tvremote") {
                        self.tv_remote = b;
                    }
                    if let Some(b) = data.get_bool("btstandby") {
                        self.bt_standby = b;
                    }
                    if let Some(b) = data.get_bool("limit_bt_conn") {
                        self.bt_restrict = b;
                    }
                }
                "NIGHT_MODE_SET" => {
                    if let Some(b) = data.get_bool("nightmode") {
                        self.night_mode = b;
                    }
                }
                "AUTO_VOL_SET" => {
                    if let Some(b) = data.get_bool("autovol") {
                        self.auto_volume = b;
                    }
                }
                "AUTO_POWER_SET" => {
                    if let Some(b) = data.get_bool("autopower") {
                        self.auto_power = b;
                    }
                }
                "AUTO_DISPLAY_SET" => {
                    if let Some(b) = data.get_bool("autodisplay") {
                        self.auto_display = b;
                    }
                }
                "DRC_SET" => {
                    if let Some(b) = data.get_bool("drc") {
                        self.drc = b;
                    }
                }
                "TV_REMOTE_SET" => {
                    if let Some(b) = data.get_bool("tvremote") {
                        self.tv_remote = b;
                    }
                }
                "LED_SET" => {
                    if let Some(b) = data.get_bool("stat").or_else(|| data.get_bool("ledset")) {
                        self.led_indicator = b;
                    }
                }
                "BT_STANDBY_SET" | "BT_STANDBY_STATE_NOTI" => {
                    if let Some(b) = data.get_bool("on").or_else(|| data.get_bool("btstandby")) {
                        self.bt_standby = b;
                    }
                }
                "BT_LIMIT_SET" | "BT_LIMIT_STATE_NOTI" => {
                    if let Some(b) = data.get_bool("limit_bt_conn") {
                        self.bt_restrict = b;
                    }
                }
                "AV_SYNC_SET" => {
                    if let Some(v) = data.get_i64("avsync") {
                        self.av_sync_ms = v;
                    }
                }
                "WOOFER_LEVEL_SET" => {
                    if let Some(w) = data.get_i64("wooferlevel") {
                        self.woofer_raw = w;
                        self.woofer_level = w + self.woofer_offset;
                    }
                }
                "REARBOX_LEVEL_SET" => {
                    if let Some(v) = data.get_i64("rearboxlevel") {
                        self.rear_level = v;
                    }
                }
                "ON_SURROUND_SET" => {
                    if let Some(b) = data.get_bool("rearboxon") {
                        self.rear_enabled = b;
                    }
                }
                "SLEEP_INFO" | "SLEEP_INFO_REQ" | "SLEEP_SET" => {
                    if let Some(t) = data.get_i64("time") {
                        self.sleep_timer_minutes = t;
                    }
                }
                "ALARM_INFO" | "ALARM_LIST_REQ" | "ALARM_SET" => {
                    if let Some(arr) = data.get_arr("info").or_else(|| data.get_arr("alarmlist")) {
                        let mut parsed_alarms = Vec::new();
                        for item in arr {
                            let id = item.get_i64("id").unwrap_or(0);
                            let mask = item.get_i64("day").unwrap_or(124) as u8;
                            let alarm_type = item.get_i64("type").unwrap_or(0);
                            let songpath = item
                                .get_str("path")
                                .or_else(|| item.get_str("songpath"))
                                .unwrap_or("")
                                .to_string();
                            let source_type = if songpath.starts_with("spotify:") {
                                "spotify"
                            } else if alarm_type == 0 && songpath.is_empty() {
                                "default_chime"
                            } else {
                                "local_stream"
                            };
                            parsed_alarms.push(WifiAlarmEntry {
                                id,
                                title: item.get_str("title").unwrap_or("SJ8 Alarm").to_string(),
                                hour: item.get_i64("hour").unwrap_or(7),
                                minute: item.get_i64("minute").unwrap_or(30),
                                days_bitmask: mask,
                                days_list: bitmask_to_days_list(mask),
                                day_repeat: item.get_bool("dayrepeat").unwrap_or(true),
                                volume: item.get_i64("volume").unwrap_or(15),
                                duration_minutes: item.get_i64("duration").unwrap_or(30),
                                enable: item.get_bool("enable").unwrap_or(true),
                                shuffle: item.get_bool("shuffle").unwrap_or(false),
                                source_type: source_type.to_string(),
                                spotify_uri: if songpath.starts_with("spotify:") {
                                    songpath.clone()
                                } else {
                                    "spotify:playlist:37i9dQZF1DXbITWG1ZJKYt".to_string()
                                },
                                local_track_id: "track-sunrise-flac".to_string(),
                                target_input: 0,
                                fade_in_seconds: 15,
                                synced_to_hardware: true,
                            });
                        }
                        self.alarms = parsed_alarms;
                    }
                }
                _ => {}
            }
        }
        self.broadcast_state();
    }

    pub fn soundbar_state_json(&self) -> String {
        format!(
            r#"{{"connected":{},"connecting":false,"host":{},"port":{},"dialect":{},"detectedDialect":"musicflow","encryption":{},"isSimulator":{},"lastUpdated":{},"name":{},"modelName":{},"petName":{},"modelNum":14,"region":"SG","protoVer":1,"registered":true,"firmwareVersion":{},"micomVersion":{},"hdmiVersion":{},"c4aVersion":{},"macAddress":{},"bluetoothMac":{},"btMacAddress":{},"lanMacAddress":{},"ipv4Addr":"192.168.0.10","ipv6Addr":"fe80::7e1c:4eff:fe3b:b022","ssid":{},"rssi":{},"meshChannel":0,"networkMode":5,"networkType":"Wireless","miracast":"off","radioPartStatus":{},"currentFunction":{},"cpType":{},"availableFunctions":[0,15,1,12,6,5,8,14],"supportedFunctions":[0,15,1,12,6,5,8,14],"customFunctionNames":{{"15":"OPTICAL/HDMI ARC"}},"volume":{},"maxVolume":{},"muted":{},"currentEq":{},"availableEqs":[14,0,7,12],"supportedEqs":[14,0,7,12,1,2,3,4,5,6,8,9,10,11,13,15,16,17,18],"bass":{},"treble":{},"lrBalance":{},"balance":{},"nightMode":{},"autoVolume":{},"drc":{},"neuralX":{},"avSyncMs":{},"avSyncMin":0,"avSyncMax":300,"wooferConnected":true,"wooferLevel":{},"wooferRaw":{},"wooferOffset":{},"wooferMin":-15,"wooferMax":6,"rearConnected":false,"rearEnabled":{},"rearLevel":{},"rearMin":-6,"rearMax":6,"centerLevel":{},"topLevel":{},"tvRemote":{},"autoPower":{},"autoDisplay":{},"btStandby":{},"btRestrict":{},"ledIndicator":{},"limitGroupGcast":false,"enableBalance":true,"visibleToneCtrl":true,"visibleAlarm":true,"visibleReserveSleep":true,"visibleMlibSync":false,"visibleTvConn":true,"sleepTimerMinutes":{},"playInfo":{{"playing":{},"isPlaying":{},"title":{},"artist":{},"album":{},"albumArt":"","sourceApp":{},"positionSec":{},"durationSec":{},"repeat":{},"shuffle":{},"cpType":{},"audioFormat":{},"audioCodec":{}}},"alarms":{},"alarmActiveNow":false,"meshNodes":{},"packetLogs":{}}}"#,
            self.connected,
            json_str(&self.host),
            self.port,
            json_str(&self.dialect),
            json_str(self.encryption.as_str()),
            self.is_simulator,
            json_str(&iso_timestamp_now()),
            json_str(&self.name),
            json_str(&self.model_name),
            json_str(&self.pet_name),
            json_str(&self.firmware_version),
            json_str(&self.micom_version),
            json_str(&self.hdmi_version),
            json_str(&self.c4a_version),
            json_str(&self.mac_address),
            json_str(&self.bt_mac_address),
            json_str(&self.bt_mac_address),
            json_str(&self.lan_mac_address),
            json_str(&self.ssid),
            self.rssi,
            self.radio_part_status,
            self.current_function,
            self.cp_type,
            self.volume,
            self.max_volume,
            self.muted,
            self.current_eq,
            self.bass,
            self.treble,
            self.balance,
            self.balance,
            self.night_mode,
            self.auto_volume,
            self.drc,
            self.neural_x,
            self.av_sync_ms,
            self.woofer_level,
            self.woofer_raw,
            self.woofer_offset,
            self.rear_enabled,
            self.rear_level,
            self.center_level,
            self.top_level,
            self.tv_remote,
            self.auto_power,
            self.auto_display,
            self.bt_standby,
            self.bt_restrict,
            self.led_indicator,
            self.sleep_timer_minutes,
            self.play_playing,
            self.play_playing,
            json_str(&self.play_title),
            json_str(&self.play_artist),
            json_str(&self.play_album),
            json_str(&self.play_source_app),
            self.play_pos_sec,
            self.play_dur_sec,
            self.play_repeat,
            self.play_shuffle,
            self.cp_type,
            json_str(&self.play_codec),
            json_str(&self.play_codec),
            self.alarms_array_json(),
            self.mesh_nodes_array_json(),
            self.packet_logs_array_json()
        )
    }

    pub fn spotify_state_json(&self) -> String {
        let is_playing = if self.connected {
            self.play_playing
        } else {
            self.spotify_playing
        };
        let max_vol = if self.max_volume > 0 { self.max_volume } else { 40 };
        let vol_pct = ((self.volume * 100) + (max_vol / 2)) / max_vol;
        format!(
            r#"{{"connected":true,"clientId":{},"displayName":"EsDeeKid (Spotify Connect)","email":"musicflow@open-audio.local","product":"premium","activeDeviceId":"lg-sj8-spotify-connect","activeDeviceName":{},"isPlaying":{},"currentTrack":{{"name":{},"artist":{},"album":{},"durationMs":{},"progressMs":{},"uri":"spotify:track:4uLU6hMCjMI75M1A2tKUQC"}},"devices":[{{"id":"lg-sj8-spotify-connect","name":"Sound Bar SJ8 (LG Music Flow TCP 9741)","type":"Speaker","is_active":true,"volume_percent":{},"isSoundbarMatch":true}},{{"id":"lg-sj8-cast","name":"Sound Bar SJ8 (Google Cast C4A)","type":"CastAudio","is_active":false,"volume_percent":{},"isSoundbarMatch":true}}],"playlists":[{{"id":"pl-morning","name":"Morning Acoustic Flow","uri":"spotify:playlist:37i9dQZF1DXbITWG1ZJKYt","trackCount":50,"description":"Gentle acoustic wakeup tracks synced with LG Music Flow Wi-Fi Alarms"}},{{"id":"pl-cinema","name":"Dolby Atmos & Bass Blast Showcase","uri":"spotify:playlist:37i9dQZF1DWZeKCadgRdKQ","trackCount":42,"description":"Deep LFE & spatial surround test tracks for LG SJ8"}},{{"id":"pl-focus","name":"Deep Coding & Hi-Res Instrumental","uri":"spotify:playlist:37i9dQZF1DX4sWSpwq3LiO","trackCount":85,"description":"Ambient piano and electronic textures"}}]}}"#,
            json_str(&self.spotify_client_id),
            json_str(&self.pet_name),
            is_playing,
            json_str(&self.play_title),
            json_str(&self.play_artist),
            json_str(&self.play_album),
            self.play_dur_sec * 1000,
            self.play_pos_sec * 1000,
            vol_pct,
            vol_pct
        )
    }

    pub fn media_tracks_json() -> &'static str {
        r#"[{"id":"track-sunrise-flac","title":"Horizon Warmup (24-bit Studio Master)","artist":"Open MusicFlow Acoustic Lab","album":"Soundbar Reference Suite","format":"WAV / FLAC","sampleRate":"44.1 kHz","bitDepth":"16-bit","durationSec":6,"moodCategory":"relaxing","streamPath":"/stream/track-sunrise-flac.wav","isBuiltInSynth":true},{"id":"track-subwoofer-sweep","title":"Subwoofer LFE 45Hz–90Hz Calibration Pulse","artist":"Open MusicFlow Acoustic Lab","album":"Cinema Calibration","format":"WAV / PCM","sampleRate":"44.1 kHz","bitDepth":"16-bit","durationSec":6,"moodCategory":"cinema","streamPath":"/stream/track-subwoofer-sweep.wav","isBuiltInSynth":true},{"id":"track-night-ambient","title":"Midnight Rain & Warm Pad","artist":"Open MusicFlow Acoustic Lab","album":"Night Whisper Sessions","format":"WAV / FLAC","sampleRate":"44.1 kHz","bitDepth":"16-bit","durationSec":6,"moodCategory":"night","streamPath":"/stream/track-night-ambient.wav","isBuiltInSynth":true},{"id":"track-focus-pulse","title":"Binaural Clarity Chime (528 Hz)","artist":"Open MusicFlow Acoustic Lab","album":"Focus Station","format":"WAV / PCM","sampleRate":"44.1 kHz","bitDepth":"16-bit","durationSec":6,"moodCategory":"focus","streamPath":"/stream/track-focus-pulse.wav","isBuiltInSynth":true}]"#
    }

    pub fn full_api_state_json(&self) -> String {
        format!(
            r#"{{"soundbar":{},"spotify":{},"tracks":{},"packetLogs":{},"dmsPort":49252}}"#,
            self.soundbar_state_json(),
            self.spotify_state_json(),
            Self::media_tracks_json(),
            self.packet_logs_array_json()
        )
    }
}

pub fn connect_soundbar_tcp(
    controller: &Arc<Mutex<SoundbarController>>,
    host: &str,
    port: u16,
    encryption: EncryptionMode,
    is_simulator: bool,
) -> bool {
    let addr_str = format!("{}:{}", host, port);
    let sock_addr: SocketAddr = match addr_str.parse() {
        Ok(a) => a,
        Err(_) => return false,
    };

    let stream = match TcpStream::connect_timeout(&sock_addr, Duration::from_millis(1500)) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let my_gen = {
        if let Ok(mut guard) = controller.lock() {
            if let Some(old_stream) = guard.tcp_writer.take() {
                let _ = old_stream.shutdown(std::net::Shutdown::Both);
            }
            guard.connection_gen = guard.connection_gen.wrapping_add(1);
            let gen = guard.connection_gen;
            guard.connected = true;
            guard.host = host.to_string();
            guard.port = port;
            guard.encryption = encryption;
            guard.is_simulator = is_simulator;
            guard.tcp_writer = Some(stream);
            guard.query_initial_sj8_state();
            guard.broadcast_state();
            gen
        } else {
            return false;
        }
    };

    let ctrl_clone = Arc::clone(controller);
    thread::spawn(move || {
        let mut reader = reader_stream;
        let mut framer = LgPacketFramer::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            let frames = framer.push(&buf[..n]);
            if let Ok(mut guard) = ctrl_clone.lock() {
                if guard.connection_gen != my_gen {
                    break;
                }
                for frame in frames {
                    guard.handle_incoming_frame(
                        frame.encrypted,
                        frame.cipher_mode,
                        frame.raw_byte_length,
                        &frame.json_text,
                    );
                }
            }
        }
        if let Ok(mut guard) = ctrl_clone.lock() {
            if guard.connection_gen == my_gen {
                guard.connected = false;
                guard.tcp_writer = None;
                guard.broadcast_state();
            }
        }
    });

    true
}

pub fn probe_soundbar_port(host: &str, port: u16, timeout_ms: u64) -> bool {
    if let Ok(addr) = format!("{}:{}", host, port).parse::<SocketAddr>() {
        TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)).is_ok()
    } else {
        false
    }
}

pub fn detect_local_lan_ip(target_host: &str) -> String {
    let probe_target = if target_host.is_empty() || target_host == "127.0.0.1" {
        "192.168.0.10:9741".to_string()
    } else {
        format!("{}:9741", target_host)
    };
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect(&probe_target).is_ok() {
            if let Ok(local) = sock.local_addr() {
                let ip = local.ip().to_string();
                if !ip.starts_with("127.") && ip != "0.0.0.0" {
                    return ip;
                }
            }
        }
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(local) = sock.local_addr() {
                let ip = local.ip().to_string();
                if !ip.starts_with("127.") && ip != "0.0.0.0" {
                    return ip;
                }
            }
        }
    }
    "127.0.0.1".to_string()
}

pub fn discover_ssdp_multicast(timeout_ms: u64) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let sock = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return results,
    };
    let _ = sock.set_multicast_ttl_v4(4);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(timeout_ms)));
    let targets = [
        "urn:lge-com:device:MultiroomSpk:1",
        "urn:schemas-upnp-org:device:MediaRenderer:1",
        "ssdp:all",
    ];
    for st in targets {
        let msearch = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: {}\r\n\r\n",
            st
        );
        let _ = sock.send_to(msearch.as_bytes(), "239.255.255.250:1900");
    }

    let mut buf = [0u8; 2048];
    while let Ok((n, src)) = sock.recv_from(&mut buf) {
        let ip = src.ip().to_string();
        let text = String::from_utf8_lossy(&buf[..n]);
        if !results.iter().any(|(existing_ip, _)| existing_ip == &ip) {
            results.push((ip, text.lines().next().unwrap_or("SSDP MediaRenderer").to_string()));
        }
    }
    results
}

pub fn send_one_shot_lg_command(
    host: &str,
    port: u16,
    json_payload: &str,
    mode: EncryptionMode,
) -> Result<Vec<String>, String> {
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .map_err(|e| format!("Invalid address: {}", e))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(2500))
        .map_err(|e| format!("Connection timeout to {}:{}: {}", host, port, e))?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1500)));

    let pkt = encode_lg_packet(json_payload, mode);
    stream
        .write_all(&pkt)
        .map_err(|e| format!("Write error: {}", e))?;

    let mut framer = LgPacketFramer::new();
    let mut buf = [0u8; 4096];
    let mut replies = Vec::new();
    while let Ok(n) = stream.read(&mut buf) {
        if n == 0 {
            break;
        }
        for f in framer.push(&buf[..n]) {
            replies.push(f.json_text);
        }
        if !replies.is_empty() {
            break;
        }
    }
    Ok(replies)
}

pub fn encode_ws_text_frame(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut frame = Vec::with_capacity(10 + len);
    frame.push(0x81); // FIN + Text opcode
    if len < 126 {
        frame.push(len as u8);
    } else if len <= 65535 {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    frame.extend_from_slice(bytes);
    frame
}

pub fn synthesize_wav_track(track_id: &str) -> Vec<u8> {
    let sample_rate: u32 = 44100;
    let duration_sec: u32 = 4;
    let num_samples = sample_rate * duration_sec;
    let data_bytes = num_samples * 2; // 16-bit mono
    let file_size = 36 + data_bytes;

    let base_freq: f64 = if track_id.contains("subwoofer") {
        65.0
    } else if track_id.contains("night") {
        220.0
    } else if track_id.contains("focus") {
        528.0
    } else {
        432.0
    };

    let mut wav = Vec::with_capacity((44 + data_bytes) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // 1 channel
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());

    for i in 0..num_samples {
        let t = (i as f64) / (sample_rate as f64);
        let env = (t * 2.0).min(1.0) * ((duration_sec as f64 - t) * 2.0).clamp(0.0, 1.0);
        let s1 = (2.0 * std::f64::consts::PI * base_freq * t).sin();
        let s2 = 0.35 * (2.0 * std::f64::consts::PI * (base_freq * 1.5) * t).sin();
        let sample = ((s1 + s2) * 0.5 * env * 22000.0) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

pub fn iso_timestamp_now() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let millis = dur.subsec_millis();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = (secs / 86400) as i64;
    // Civil date from days since 1970-01-01 (Howard Hinnant algorithm)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if mo <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, mo, d, h, m, s, millis
    )
}
