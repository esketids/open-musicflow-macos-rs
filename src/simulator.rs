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
    ledset: bool,
    tvremote: bool,
    btstandby: bool,
    limit_bt_conn: bool,
    sleeptime: i64,
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
            ledset: true,
            tvremote: true,
            btstandby: true,
            limit_bt_conn: false,
            sleeptime: 0,
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
                r#"{{"id":{},"title":{},"hour":{},"minute":{},"day":{},"dayrepeat":{},"volume":{},"duration":{},"type":{},"enable":{},"shuffle":{},"ipaddress":{},"songpath":{}}}"#,
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
                r#"{{"msg":"PRODUCT_INFO","result":"ok","data":{{"info":{{"ip":"192.168.0.10","mac":"7c:1c:4e:3b:b0:22","modelname":"SJ8","modelnum":14,"name":{},"network":2,"petname":{},"protoVer":1,"region":"SG"," registered":true,"role":"none"}}}}}}"#,
                json_str(&st.name),
                json_str(&st.petname)
            ),
            r#"{"msg":"SYSTEM_VER_INFO","result":"ok","data":{"system":"NB9.504.00629.C","micom":"1904080","hdmi":"1807100","c4a":"1.36.145853"}}"#.to_string(),
        ],
        "RADIO_PART_CONN_GET" => vec![
            r#"{"msg":"RADIO_PART_CONN_RESP","result":"ok","data":{"status":1}}"#.to_string(),
        ],
        "MAC_INFO_REQ" | "MAC_INFO_DEV" => vec![
            r#"{"msg":"MAC_INFO_DEV","result":"ok","data":{"btmac":"08:EF:3B:9C:2D:4F","lanmac":"00:e0:4c:36:14:98","wifimac":"7c:1c:4e:3b:b0:22"}}"#.to_string(),
        ],
        "NET_INFO_REQ" => vec![
            r#"{"msg":"NET_INFO_DEV","result":"ok","data":{"meshch":0,"miracast":"off"," mode":5,"role":0,"rssi":3,"ssid":"Adnan_Home_WiFi"}}"#.to_string(),
        ],
        "EQ_INFO_REQ" => vec![format!(
            r#"{{"msg":"EQ_INFO","result":"ok","data":{{"eq":{},"eqcount":4,"eqlist":[14,0,7,12],"bass":{},"treble":{},"balance":{}}}}}"#,
            st.eq, st.bass, st.treble, st.balance
        )],
        "SETTING_INFO_REQ" => vec![format!(
            r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"Audio Guide":false,"auxcable":false,"display":true,"nightmode":{},"autovol":{},"drc":{},"avsync":{},"avsyncmax":300,"avsyncmin":0,"rearbox":false,"rearboxlevel":{},"rearboxmax":6,"rearboxmin":-6,"rearboxoffset":0,"sleep":{},"tvremote":{},"visible_alarm":true,"visible_autopower":true,"visible_avsync":true,"visible_btlimit":true,"visible_btstandby":true,"visible_led":true,"visible_nightmode":true,"visible_SIMPLINK":false,"visible_tvremote":true,"visible_woofer":true,"woofer":true,"wooferlevel":{},"woofermax":21,"woofermin":0,"wooferoffset":-15,"autopower":{},"btstandby":{},"limit_bt_conn":{},"ledset":{}}}}}"#,
            st.nightmode,
            st.autovol,
            st.drc,
            st.avsync,
            st.rearboxlevel,
            st.sleeptime,
            st.tvremote,
            st.wooferlevel,
            st.autopower,
            st.btstandby,
            st.limit_bt_conn,
            st.ledset
        )],
        "PLAY_INFO" => vec![format!(
            r#"{{"msg":"PLAY_INFO","result":"ok","data":{{"album":{},"artist":{},"cpname":{},"duration":{},"icon":2,"mute":{},"playstatus":"play","position":{},"repeat":0,"shuffle":true,"support":191,"title":{},"type":1,"volume":{},"maxvolume":40}}}}"#,
            json_str(&st.play_album),
            json_str(&st.play_artist),
            json_str(&st.play_cpname),
            st.play_dur,
            st.mute,
            st.play_pos,
            json_str(&st.play_title),
            st.volume
        )],
        "FUNC_INFO_REQ" => vec![format!(
            r#"{{"msg":"FUNC_INFO","result":"ok","data":{{"cptype":2,"functionlist":[0,15,1,12,6,5],"sub_type":0,"type":{}}}}}"#,
            st.function
        )],
        "FUNC_NAME_REQ" => vec![
            r#"{"msg":"FUNC_NAME_INFO","result":"ok","data":{"func":15,"name":"OPTICAL/HDMI ARC"}}"#.to_string(),
        ],
        "ALARM_STATE_REQ" => vec![
            r#"{"msg":"ALARM_STATE","result":"ok","data":{"state":"ok"}}"#.to_string(),
        ],
        "ALARM_LIST_REQ" => vec![format!(
            r#"{{"msg":"ALARM_INFO","result":"ok","data":{{"alarmlist":{}}}}}"#,
            st.alarms_json()
        )],
        "SLEEP_INFO_REQ" => vec![format!(
            r#"{{"msg":"SLEEP_INFO","result":"ok","data":{{"time":{}}}}}"#,
            st.sleeptime
        )],
        "VOLUME_SETTING" => {
            if let Some(d) = data {
                if let Some(v) = d.get_i64("volume") {
                    st.volume = v.clamp(0, 40);
                }
            }
            vec![format!(
                r#"{{"msg":"VOLUME_INFO","result":"ok","data":{{"volume":{},"maxvolume":40,"mute":{}}}}}"#,
                st.volume, st.mute
            )]
        }
        "MUTE_SET" => {
            if let Some(d) = data {
                if let Some(m) = d.get_bool("mute") {
                    st.mute = m;
                }
            }
            vec![format!(
                r#"{{"msg":"VOLUME_INFO","result":"ok","data":{{"volume":{},"maxvolume":40,"mute":{}}}}}"#,
                st.volume, st.mute
            )]
        }
        "FUNCTION_SET" => {
            if let Some(d) = data {
                if let Some(f) = d.get_i64("func") {
                    st.function = f;
                }
            }
            vec![format!(
                r#"{{"msg":"FUNC_INFO","result":"ok","data":{{"cptype":2,"functionlist":[0,15,1,12,6,5],"sub_type":0,"type":{}}}}}"#,
                st.function
            )]
        }
        "EQ_SETTING" => {
            if let Some(d) = data {
                if let Some(eq) = d.get_i64("eq") {
                    st.eq = eq;
                }
                if let Some(t) = d.get_i64("type") {
                    let v = d.get_i64("value").unwrap_or(0);
                    match t {
                        1 => st.bass = v,
                        2 => st.treble = v,
                        3 => st.balance = v,
                        _ => {}
                    }
                }
            }
            vec![format!(
                r#"{{"msg":"EQ_INFO","result":"ok","data":{{"eq":{},"eqcount":4,"eqlist":[14,0,7,12],"bass":{},"treble":{},"balance":{}}}}}"#,
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
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"wooferlevel":{},"wooferoffset":-15,"woofermax":21}}}}"#,
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
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"nightmode":{}}}}}"#,
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
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"autovol":{}}}}}"#,
                st.autovol
            )]
        }
        "DRC_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("drc") {
                    st.drc = b;
                }
            }
            vec![format!(
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"drc":{}}}}}"#,
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
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"avsync":{}}}}}"#,
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
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"rearboxlevel":{}}}}}"#,
                st.rearboxlevel
            )]
        }
        "AUTO_POWER_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("autopower") {
                    st.autopower = b;
                }
            }
            vec![format!(
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"autopower":{}}}}}"#,
                st.autopower
            )]
        }
        "LED_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("ledset") {
                    st.ledset = b;
                }
            }
            vec![format!(
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"ledset":{}}}}}"#,
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
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"tvremote":{}}}}}"#,
                st.tvremote
            )]
        }
        "BT_STANDBY_SET" => {
            if let Some(d) = data {
                if let Some(b) = d.get_bool("btstandby") {
                    st.btstandby = b;
                }
            }
            vec![format!(
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"btstandby":{}}}}}"#,
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
                r#"{{"msg":"SETTING_INFO","result":"ok","data":{{"limit_bt_conn":{}}}}}"#,
                st.limit_bt_conn
            )]
        }
        "SLEEP_SET" => {
            if let Some(d) = data {
                if let Some(v) = d.get_i64("time") {
                    st.sleeptime = v;
                }
            }
            vec![format!(
                r#"{{"msg":"SLEEP_INFO","result":"ok","data":{{"time":{}}}}}"#,
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
                r#"{{"msg":"ALARM_INFO","result":"ok","data":{{"alarmlist":{},"mode":0,"result":0}}}}"#,
                st.alarms_json()
            )]
        }
        "SET_ALARM_PLAYLIST" => vec![
            r#"{"msg":"SET_ALARM_PLAYLIST","result":"ok","data":{"result":0}}"#.to_string(),
        ],
        "SHARE_HOME_INFO" => vec![
            r#"{"msg":"SHARE_HOME_INFO","result":"ok","data":{"result":0,"status":"ok"}}"#.to_string(),
        ],
        "SPK_ADD_SET" => vec![
            r#"{"msg":"SPK_ADD_RESULT","result":"ok","data":{"error_code":0,"result":0}}"#.to_string(),
        ],
        _ => vec![format!(r#"{{"msg":{},"result":"ok"}}"#, json_str(msg))],
    }
}
