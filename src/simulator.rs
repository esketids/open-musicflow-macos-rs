//! Pure-Rust Virtual LG SJ8 TCP 9741 Hardware Simulator.
//! Speaks real AES-256-CBC (`sj8_v1` / `temescal_v2`) and `0x00` plaintext frames.

use crate::crypto::{encode_lg_packet, LgPacketFramer};
use crate::json::{json_str, parse_json, JsonValue};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
struct SimAlarm {
    id: i64,
    title: String,
    hour: i64,
    minute: i64,
    day: i64,
    dayrepeat: bool,
    volume: i64,
    duration: i64,
    alarm_type: i64,
    enable: bool,
    shuffle: bool,
    ipaddress: String,
    songpath: String,
}

struct SimState {
    name: String,
    petname: String,
    volume: i64,
    mute: bool,
    function: i64,
    eq: i64,
    bass: i64,
    treble: i64,
    balance: i64,
    nightmode: bool,
    autovol: bool,
    drc: bool,
    avsync: i64,
    wooferlevel: i64, // Raw 0..21 (offset -15 => -15..+6 dB)
    rearboxlevel: i64,
    autopower: bool,
    autodisplay: bool,
    ledset: bool,
    tvremote: bool,
    btstandby: bool,
    limit_bt_conn: bool,
    rearboxon: bool,
    sleeptime: i64,
    ssid: String,
    playing: i64, // 0=PLAY, 1=PAUSE, 2=STOP, 3=FULL_STOP, 4=BUFFERING
    play_repeat: i64,
    play_shuffle: bool,
    play_title: String,
    play_artist: String,
    play_album: String,
    play_cpname: String,
    play_pos: i64,
    play_dur: i64,
    alarms: Vec<SimAlarm>,
}

impl SimState {
    fn new() -> Self {
        Self {
            name: "LG SJ8".to_string(),
            petname: "Sound Bar SJ8".to_string(),
            volume: 10,
            mute: false,
            function: 8, // Spotify Connect
            eq: 12,      // ASC (Adaptive Sound Control)
            bass: 0,
            treble: 0,
            balance: 0,
            nightmode: false,
            autovol: false,
            drc: true,
            avsync: 0,
            wooferlevel: 15, // 15 + (-15) = 0 dB
            rearboxlevel: 0,
            autopower: true,
            autodisplay: true,
            ledset: true,
            tvremote: true,
            btstandby: true,
            limit_bt_conn: false,
            rearboxon: false,
            sleeptime: 0,
            ssid: "Adnan_Home_WiFi".to_string(),
            playing: 0,
            play_repeat: 0,
            play_shuffle: true,
            play_title: "Bandit".to_string(),
            play_artist: "EsDeeKid".to_string(),
            play_album: "Bandit - Single".to_string(),
            play_cpname: "Spotify".to_string(),
            play_pos: 49,
            play_dur: 136,
            alarms: vec![
                SimAlarm {
                    id: 0,
                    title: "Weekday SJ8 Built-in Chime".to_string(),
                    hour: 7,
                    minute: 30,
                    day: 124,
                    dayrepeat: true,
                    volume: 12,
                    duration: 30,
                    alarm_type: 0,
                    enable: true,
                    shuffle: false,
                    ipaddress: "".to_string(),
                    songpath: "".to_string(),
                },
                SimAlarm {
                    id: 1,
                    title: "Weekend Local FLAC Wakeup".to_string(),
                    hour: 9,
                    minute: 0,
                    day: 3,
                    dayrepeat: true,
                    volume: 15,
                    duration: 45,
                    alarm_type: 1,
                    enable: false,
                    shuffle: true,
                    ipaddress: "192.168.1.176".to_string(),
                    songpath: "http://192.168.1.176:49252/stream/track-sunrise-flac.wav".to_string(),
                },
            ],
        }
    }

    fn alarms_json(&self) -> String {
        let mut items = Vec::new();
        for a in &self.alarms {
            items.push(format!(
                r#"{{"id":{},"title":{},"hour":{},"minute":{},"day":{},"dayrepeat":{},"volume":{},"duration":{},"type":{},"enable":{},"shuffle":{},"ipaddress":{},"path":{},"songpath":{}}}"#,
                a.id,
                json_str(&a.title),
                a.hour,
                a.minute,
                a.day,
                a.dayrepeat,
                a.volume,
                a.duration,
                a.alarm_type,
                a.enable,
                a.shuffle,
                json_str(&a.ipaddress),
                json_str(&a.songpath),
                json_str(&a.songpath),
            ));
        }
        format!("[{}]", items.join(","))
    }
}

pub fn start_mock_soundbar_server(preferred_port: u16) -> u16 {
    let mut port = preferred_port;
    let listener = loop {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(l) => break l,
            Err(_) => {
                port += 1;
            }
        }
    };

    let state = Arc::new(Mutex::new(SimState::new()));
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let st = Arc::clone(&state);
            thread::spawn(move || handle_sim_client(stream, st));
        }
    });
    port
}

fn handle_sim_client(mut stream: TcpStream, state: Arc<Mutex<SimState>>) {
    let mut framer = LgPacketFramer::new();
    let mut buf = [0u8; 4096];

    while let Ok(n) = stream.read(&mut buf) {
        if n == 0 {
            break;
        }
        let frames = framer.push(&buf[..n]);
        for frame in frames {
            let reply_mode = frame.cipher_mode;
            if let Some(parsed) = parse_json(&frame.json_text) {
                let msg = parsed.get_str("msg").unwrap_or("");
                let data = parsed.get("data");
                let replies = build_sim_replies(msg, data, &state);
                for rep in replies {
                    let pkt = encode_lg_packet(&rep, reply_mode);
                    let _ = stream.write_all(&pkt);
                }
            }
        }
    }
}

fn build_sim_replies(
    msg: &str,
    data: Option<&JsonValue>,
    state: &Arc<Mutex<SimState>>,
) -> Vec<String> {
    let mut st = match state.lock() {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };

    match msg {
        "PRODUCT_INFO" => vec![
            format!(
                r#"{{"msg":"PRODUCT_INFO","result":"OK","data":{{"reg":true,"network":1,"protover":1,"region":"SG","modeltype":2,"petname":{},"modelname":"SJ8","modelnum":14,"info":{{"name":{},"icon":0,"groupid":0,"ledset":{},"channel":0,"groupcolor":0,"function":{},"spktype":0,"playing":{},"vol":{},"wirelessmac":"7c:1c:4e:3b:b0:22","mute":{},"update":false,"eqlist":[14,0,7,12],"functionlist":[0,15,1,12,6,5,8],"demo":false,"bever":"NB9.504.00629.C","btmac":"08:EF:3B:9C:2D:4F"}}}}}}"#,
                json_str(&st.petname),
                json_str(&st.name),
                st.ledset,
                st.function,
                st.playing == 0 || st.playing == 4,
                st.volume,
                st.mute,
            ),
        ],
        "SYSTEM_VER_REQ" => vec![
            r#"{"msg":"SYSTEM_VER_REQ","result":"OK","data":{"be":"NB9.504.00629.C","micom":"1904080","hdmi":"1807100","c4a":"1.36.145853"}}"#.to_string(),
        ],
        "RADIO_PART_CONN_GET" => vec![
            r#"{"msg":"RADIO_PART_CONN_RESP","result":"OK","data":{"status":1}}"#.to_string(),
        ],
        "MAC_INFO_REQ" | "MAC_INFO_DEV" => vec![
            r#"{"msg":"MAC_INFO_DEV","result":"OK","data":{"btmac":"08:EF:3B:9C:2D:4F","lanmac":"00:e0:4c:36:14:98","wifimac":"7c:1c:4e:3b:b0:22"}}"#.to_string(),
        ],
        "NETWORK_INFO_REQ" | "NET_INFO_REQ" => vec![format!(
            r#"{{"msg":"NETWORK_INFO_REQ","result":"OK","data":{{"meshch":0,"network":1,"ssid":{}}}}}"#,
            json_str(&st.ssid)
        )],
        "EQ_INFO_REQ" => vec![format!(
            r#"{{"msg":"EQ_INFO_REQ","result":"OK","data":{{"currenteq":{},"bass":{},"treble":{},"lrbal":{}}}}}"#,
            st.eq, st.bass, st.treble, st.balance
        )],
        "SETTING_INFO_REQ" => vec![format!(
            r#"{{"msg":"SETTING_INFO_REQ","result":"OK","data":{{"autodisplay":{},"nightmode":{},"autovol":{},"drc":{},"avsync":{},"rearboxon":{},"rearboxlevel":{},"rearboxoffset":0,"tvremote":{},"visible_alarm":true,"visible_tonectrl":true,"wooferlevel":{},"woofermax":21,"wooferoffset":-15,"autopower":{},"btstandby":{},"limit_bt_conn":{},"ledset":{},"ipv4addr":"192.168.0.10"}}}}"#,
            st.autodisplay,
            st.nightmode,
            st.autovol,
            st.drc,
            st.avsync,
            st.rearboxon,
            st.rearboxlevel,
            st.tvremote,
            st.wooferlevel,
            st.autopower,
            st.btstandby,
            st.limit_bt_conn,
            st.ledset
        )],
        "PLAY_INFO" | "PLAY_INFO_REQ" | "CP_INFO_REQ" | "CP_PLAY_URL" | "LOCAL_PLAY_URL" => {
            if let Some(d) = data {
                let item = d.get("item").unwrap_or(d);
                if let Some(t) = item.get_str("title") {
                    if !t.is_empty() {
                        st.play_title = t.to_string();
                    }
                }
                if let Some(a) = item.get_str("artist") {
                    if !a.is_empty() {
                        st.play_artist = a.to_string();
                    }
                }
                if let Some(al) = item.get_str("albumtitle").or_else(|| item.get_str("album")) {
                    if !al.is_empty() {
                        st.play_album = al.to_string();
                    }
                }
                if let Some(p) = d.get_i64("playing") {
                    st.playing = p;
                } else if msg == "LOCAL_PLAY_URL" || msg == "CP_PLAY_URL" {
                    st.playing = 0;
                }
            }
            let reply_msg = match msg {
                "PLAY_INFO_REQ" => "PLAY_INFO_REQ",
                "CP_INFO_REQ" => "CP_INFO_REQ",
                _ => "PLAY_INFO",
            };
            vec![format!(
                r#"{{"msg":{},"result":"OK","data":{{"albumtitle":{},"artist":{},"c4aappname":{},"duration":{},"playing":{},"position":{},"repeat":{},"shuffle":{},"title":{},"cptype":2}}}}"#,
                json_str(reply_msg),
                json_str(&st.play_album),
                json_str(&st.play_artist),
                json_str(&st.play_cpname),
                st.play_dur,
                st.playing,
                st.play_pos,
                st.play_repeat,
                st.play_shuffle,
                json_str(&st.play_title),
            )]
        }
        "PLAY_CMD" => {
            let mut ctrl = 0i64;
            if let Some(d) = data {
                ctrl = d.get_i64("playctrl").unwrap_or(0);
                if ctrl == 0 || ctrl == 1 || ctrl == 2 {
                    st.playing = ctrl;
                } else if ctrl == 3 || ctrl == 5 {
                    st.play_pos = 0;
                }
                if let Some(rep) = d.get_i64("repeat") {
                    st.play_repeat = rep;
                }
                if let Some(shuf) = d.get_bool("shuffle") {
                    st.play_shuffle = shuf;
                }
            }
            vec![
                format!(
                    r#"{{"msg":"PLAY_CMD","result":"OK","data":{{"playctrl":{},"repeat":{},"shuffle":{}}}}}"#,
                    ctrl, st.play_repeat, st.play_shuffle
                ),
                format!(
                    r#"{{"msg":"PLAY_INFO","data":{{"albumtitle":{},"artist":{},"c4aappname":{},"duration":{},"playing":{},"position":{},"repeat":{},"shuffle":{},"title":{},"cptype":2}}}}"#,
                    json_str(&st.play_album),
                    json_str(&st.play_artist),
                    json_str(&st.play_cpname),
                    st.play_dur,
                    st.playing,
                    st.play_pos,
                    st.play_repeat,
                    st.play_shuffle,
                    json_str(&st.play_title),
                ),
            ]
        }
        "LOCAL_TIME_SEARCH" => {
            if let Some(d) = data {
                if let Some(t) = d.get_i64("time") {
                    st.play_pos = t.clamp(0, st.play_dur);
                }
            }
            vec![format!(
                r#"{{"msg":"LOCAL_TIME_SEARCH","result":"OK","data":{{"time":{}}}}}"#,
                st.play_pos
            )]
        }
        "PLAY_TIME_SET" => vec![r#"{"msg":"PLAY_TIME_SET","result":"OK"}"#.to_string()],
        "FUNC_INFO_REQ" => vec![format!(
            r#"{{"msg":"FUNC_INFO_REQ","result":"OK","data":{{"type":{},"mute":{},"btname":"","connect":{}}}}}"#,
            st.function,
            st.mute,
            if st.function == 8 { 1 } else { 0 }
        )],
        "FUNC_NAME_REQ" => vec![
            r#"{"msg":"FUNC_NAME_REQ","result":"OK","data":{"func":15,"name":"Optical/HDMI ARC"}}"#.to_string(),
        ],
        "ALARM_STATE_REQ" => vec![
            r#"{"msg":"ALARM_STATE_NOTI","data":{"on":false}}"#.to_string(),
        ],
        "ALARM_LIST_REQ" => vec![format!(
            r#"{{"msg":"ALARM_LIST_REQ","result":"OK","data":{{"info":{}}}}}"#,
            st.alarms_json()
        )],
        "SLEEP_INFO_REQ" => vec![format!(
            r#"{{"msg":"SLEEP_INFO_REQ","result":"OK","data":{{"time":{}}}}}"#,
            st.sleeptime
        )],
        "VOLUME_SETTING" => {
            if let Some(d) = data {
                if let Some(v) = d.get_i64("vol").or_else(|| d.get_i64("volume")) {
                    st.volume = v.clamp(0, 40);
                }
            }
            vec![format!(
                r#"{{"msg":"VOLUME_CHANGE","result":"OK","data":{{"vol":{}}}}}"#,
                st.volume
            )]
        }
        "VOLUME_UP" | "VOLUME_DOWN" => {
            let delta = if msg == "VOLUME_UP" { 1 } else { -1 };
            st.volume = (st.volume + delta).clamp(0, 40);
            vec![format!(
                r#"{{"msg":"VOLUME_CHANGE","result":"OK","data":{{"vol":{}}}}}"#,
                st.volume
            )]
        }
        "MUTE_SET" => {
            if let Some(d) = data {
                if let Some(m) = d.get_bool("mute") {
                    st.mute = m;
                }
            }
            vec![format!(
                r#"{{"msg":"MUTE_CHANGE","result":"OK","data":{{"mute":{}}}}}"#,
                st.mute
            )]
        }
        "FUNCTION_SET" => {
            if let Some(d) = data {
                if let Some(f) = d.get_i64("type").or_else(|| d.get_i64("func")) {
                    st.function = f;
                }
            }
            vec![format!(
                r#"{{"msg":"FUNC_INFO","result":"OK","data":{{"type":{},"mute":{},"btname":"","connect":{}}}}}"#,
                st.function,
                st.mute,
                if st.function == 8 { 1 } else { 0 }
            )]
        }
        "EQ_SETTING" => {
            if let Some(d) = data {
                if let Some(t) = d.get_i64("type") {
                    let v = d.get_i64("value").unwrap_or(0);
                    match t {
                        0 => st.eq = v,
                        1 => st.bass = v,
                        2 => st.treble = v,
                        3 => st.balance = v,
                        _ => {}
                    }
                } else if let Some(eq) = d.get_i64("eq") {
                    st.eq = eq;
                }
            }
            vec![format!(
                r#"{{"msg":"EQ_INFO_REQ","result":"OK","data":{{"currenteq":{},"bass":{},"treble":{},"lrbal":{}}}}}"#,
                st.eq, st.bass, st.treble, st.balance
            )]
        }
        "WOOFER_LEVEL_SET" => {
            if let Some(d) = data {
                if let Some(w) = d.get_i64("wooferlevel") {
                    st.wooferlevel = w.clamp(0, 21);
                }
            }
            vec![format!(
                r#"{{"msg":"WOOFER_LEVEL_SET","result":"OK","data":{{"wooferlevel":{}}}}}"#,
                st.wooferlevel
            )]
        }
        "NIGHT_MODE_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("nightmode") {
                    st.nightmode = b;
                }
            }
            vec![format!(
                r#"{{"msg":"NIGHT_MODE_SET","result":"OK","data":{{"nightmode":{}}}}}"#,
                st.nightmode
            )]
        }
        "AUTO_VOL_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("autovol") {
                    st.autovol = b;
                }
            }
            vec![format!(
                r#"{{"msg":"AUTO_VOL_SET","result":"OK","data":{{"autovol":{}}}}}"#,
                st.autovol
            )]
        }
        "AUTO_DISPLAY_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("autodisplay") {
                    st.autodisplay = b;
                }
            }
            vec![format!(
                r#"{{"msg":"AUTO_DISPLAY_SET","result":"OK","data":{{"autodisplay":{}}}}}"#,
                st.autodisplay
            )]
        }
        "DRC_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("drc") {
                    st.drc = b;
                }
            }
            vec![format!(
                r#"{{"msg":"DRC_SET","result":"OK","data":{{"drc":{}}}}}"#,
                st.drc
            )]
        }
        "AV_SYNC_SET" => {
            if let Some(d) = data {
                if let Some(v) = d.get_i64("avsync") {
                    st.avsync = v.clamp(0, 300);
                }
            }
            vec![format!(
                r#"{{"msg":"AV_SYNC_SET","result":"OK","data":{{"avsync":{}}}}}"#,
                st.avsync
            )]
        }
        "REARBOX_LEVEL_SET" => {
            if let Some(d) = data {
                if let Some(v) = d.get_i64("rearboxlevel") {
                    st.rearboxlevel = v.clamp(-6, 6);
                }
            }
            vec![format!(
                r#"{{"msg":"REARBOX_LEVEL_SET","result":"OK","data":{{"rearboxlevel":{}}}}}"#,
                st.rearboxlevel
            )]
        }
        "ON_SURROUND_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("rearboxon") {
                    st.rearboxon = b;
                }
            }
            vec![format!(
                r#"{{"msg":"ON_SURROUND_SET","result":"OK","data":{{"rearboxon":{}}}}}"#,
                st.rearboxon
            )]
        }
        "AUTO_POWER_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("autopower") {
                    st.autopower = b;
                }
            }
            vec![format!(
                r#"{{"msg":"AUTO_POWER_SET","result":"OK","data":{{"autopower":{}}}}}"#,
                st.autopower
            )]
        }
        "LED_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("stat").or_else(|| d.get_bool("ledset")) {
                    st.ledset = b;
                }
            }
            vec![format!(
                r#"{{"msg":"LED_SET","result":"OK","data":{{"stat":{}}}}}"#,
                st.ledset
            )]
        }
        "TV_REMOTE_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("tvremote") {
                    st.tvremote = b;
                }
            }
            vec![format!(
                r#"{{"msg":"TV_REMOTE_SET","result":"OK","data":{{"tvremote":{}}}}}"#,
                st.tvremote
            )]
        }
        "BT_STANDBY_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("on").or_else(|| d.get_bool("btstandby")) {
                    st.btstandby = b;
                }
            }
            vec![format!(
                r#"{{"msg":"BT_STANDBY_SET","result":"OK","data":{{"on":{}}}}}"#,
                st.btstandby
            )]
        }
        "BT_LIMIT_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("limit_bt_conn") {
                    st.limit_bt_conn = b;
                }
            }
            vec![format!(
                r#"{{"msg":"BT_LIMIT_SET","result":"OK","data":{{"limit_bt_conn":{}}}}}"#,
                st.limit_bt_conn
            )]
        }
        "SLEEP_SET" => {
            if let Some(d) = data {
                if let Some(v) = d.get_i64("time") {
                    st.sleeptime = v.max(0);
                }
            }
            vec![format!(
                r#"{{"msg":"SLEEP_SET","result":"OK","data":{{"time":{}}}}}"#,
                st.sleeptime
            )]
        }
        "ALARM_SET" => {
            if let Some(d) = data {
                let mode = d.get_i64("mode").unwrap_or(0);
                let id = d.get_i64("id").unwrap_or(-1);
                match mode {
                    2 => {
                        st.alarms.retain(|a| a.id != id);
                    }
                    3 => {
                        if let Some(a) = st.alarms.iter_mut().find(|a| a.id == id) {
                            a.enable = true;
                        }
                    }
                    4 => {
                        if let Some(a) = st.alarms.iter_mut().find(|a| a.id == id) {
                            a.enable = false;
                        }
                    }
                    _ => {
                        let mod_id = d.get_i64("modifiedid").unwrap_or_else(|| {
                            if id >= 0 {
                                id
                            } else {
                                st.alarms.iter().map(|a| a.id).max().unwrap_or(-1) + 1
                            }
                        });
                        let entry = SimAlarm {
                            id: mod_id,
                            title: d.get_str("title").unwrap_or("SJ8 Wakeup Alarm").to_string(),
                            hour: d.get_i64("hour").unwrap_or(7),
                            minute: d.get_i64("minute").unwrap_or(30),
                            day: d.get_i64("day").unwrap_or(124),
                            dayrepeat: d.get_bool("dayrepeat").unwrap_or(true),
                            volume: d.get_i64("volume").unwrap_or(15),
                            duration: d.get_i64("duration").unwrap_or(30),
                            alarm_type: d.get_i64("type").unwrap_or(0),
                            enable: d.get_bool("enable").unwrap_or(true),
                            shuffle: d.get_bool("shuffle").unwrap_or(false),
                            ipaddress: d.get_str("ipaddress").unwrap_or("").to_string(),
                            songpath: d.get_str("songpath").unwrap_or("").to_string(),
                        };
                        if let Some(existing) = st.alarms.iter_mut().find(|a| a.id == mod_id) {
                            *existing = entry;
                        } else {
                            st.alarms.push(entry);
                        }
                    }
                }
            }
            vec![format!(
                r#"{{"msg":"ALARM_SET","result":"OK","data":{{"info":{}}}}}"#,
                st.alarms_json()
            )]
        }
        "SET_ALARM_PLAYLIST" => vec![
            r#"{"msg":"SET_ALARM_PLAYLIST","result":"OK","data":{"index":0}}"#.to_string(),
        ],
        "SHARE_HOME_INFO" => {
            if let Some(d) = data {
                if let Some(ssid) = d.get_str("ssid") {
                    st.ssid = ssid.to_string();
                }
            }
            vec![format!(
                r#"{{"msg":"SHARE_HOME_INFO","result":"OK","data":{{"ssid":{}}}}}"#,
                json_str(&st.ssid)
            )]
        }
        "SPK_ADD_SET" | "SPK_INFO_MODIFY" => {
            if let Some(d) = data {
                if let Some(name) = d.get_str("name") {
                    if !name.trim().is_empty() {
                        st.name = name.trim().to_string();
                    }
                }
            }
            vec![format!(
                r#"{{"msg":{},"result":"OK","data":{{"name":{},"reg":true}}}}"#,
                json_str(msg),
                json_str(&st.name)
            )]
        }
        _ => vec![format!(r#"{{"msg":{},"result":"OK"}}"#, json_str(msg))],
    }
}
