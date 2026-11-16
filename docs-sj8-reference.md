# LG Music Flow + LG SJ8 CachyOS & OpenWrt Technical Reference Log

**Generated:** 2026-11-17 (Asia/Singapore, `+08:00`)  
**Target Workstation (`cachyos-esketids`):** CachyOS Linux `7.3.4-1-cachyos` (KDE Plasma Wayland + XWayland, `DISPLAY=:0`)  
**LAN / Tailscale IPs (`cachyos-esketids`):** `192.168.1.176` (`enp0s31f6`) / `100.65.102.108` (`tailscale0`)  
**Router (`openwrt`):** Linksys `EA7500 v2` running OpenWrt `25.12.6` (`192.168.1.1` on `br-lan`, `192.168.0.4` on `wan`; Wi-Fi radios disabled)  
**Upstream Wi-Fi AP (`HB710`):** TP-Link `HB710` (`192.168.0.1/24`, SSID `o`)  
**Provisioning Workstation (MacBook):** `192.168.0.5` (`en0` Wi-Fi) / `192.168.1.243` (`en6` Ethernet) / `100.121.25.86` (`tailscale`)  
**Soundbar:** LG `SJ8` (`Music Flow SJ8(02:B5)` / AP `MusicFlow_Setup`, `192.168.0.10`, Wi-Fi MAC `7c:1c:4e:3b:b0:22`, BT MAC `08:EF:3B:9C:2D:4F`, backend firmware `bever: NB9.504.00629.C`)  
**Source Installer:** `/Users/esketids/Downloads/Chrome Downloads/MusicFlow_Windows` -> synced to `cachyos-esketids:~/Downloads/MusicFlow_Windows` (LG Music Flow PC Software v`1.1.1611.701`, build `201611071407`, InstallShield v7 / `ProductGUID=151B17E8-6B25-4DFE-90BD-6838CC607164`)

---

## 1. Execution Platform Decision: Native Wine vs. WinBoat VM

- Checked existing containers on `cachyos-esketids`: `WinBoat` (`ghcr.io/dockur/windows:6.05`, configured with 24 GB RAM / KVM NAT networking) was `Exited (137)`. It was left untouched/stopped.
- **Why isolated native Wine (`wine-11.22` WoW64) was chosen:**
  - `MusicFlow.exe` relies on L2/L3 multicast SSDP (`239.255.255.250:1900/udp`), UDP broadcast (`9742/udp`), and direct TCP connections (`9741/tcp`, `49252/tcp`).
  - WinBoat sits behind Docker NAT, which breaks SSDP multicast & reverse DLNA callbacks, and consumes 24 GB RAM.
  - Wine on CachyOS shares the host network namespace directly on `enp0s31f6` (`192.168.1.176`), and `ufw` is disabled (`/etc/ufw/ufw.conf` -> `ENABLED=no`).

---

## 2. End-to-End Network Architecture & Packet Flow

### 2.1 Subnet & Gateway Topology

```text
[LG SJ8 Soundbar: 192.168.0.10]
       ^   |
       |   | Wi-Fi SSID 'o' (239.255.255.250:1900, 2870/udp, 9741/tcp)
       v   v
[TP-Link HB710: 192.168.0.1/24]
       ^   |
       |   | Ethernet wan (192.168.0.4)
       v   v
[OpenWrt EA7500 v2: 192.168.0.4 (wan) <-> 192.168.1.1 (br-lan)]
  - smcrouted: relays (*, 239.255.255.250) br-lan <-> wan
  - iptables mangle PREROUTING: rewrites SSDP TTL=1 -> TTL=4
  - fw4 + /etc/nftables.d/20-ssdp-reply.nft: DNAT & port-preserving reply routing
       ^   |
       |   | Ethernet br-lan (192.168.1.1/24)
       v   v
[cachyos-esketids: 192.168.1.176 (enp0s31f6)]
  - Wineserver + MusicFlow.exe + MusicFlowTray.exe + MusicFlowDMS.exe
```

### 2.2 Ports & Protocols Used by Music Flow (`MusicFlow_Windows`)

| Port / Proto | Direction | Component | Purpose |
|---|---|---|---|
| `239.255.255.250:1900/udp` | Bidirectional multicast | `MusicFlow.exe` (`MSearchTCPClient`) & `MusicFlowDMS.exe` | SSDP `M-SEARCH` (`urn:lge-com:device:MultiroomSpk:1`, `ssdp:all`) and `NOTIFY` advertisements |
| Ephemeral `1024-65535/udp` | `SJ8 (2870/udp)` -> `192.168.0.4` -> `192.168.1.176` | `MusicFlow.exe` (e.g. `43577/udp`) | Unicast `HTTP/1.1 200 OK` SSDP responses pointing to `http://192.168.0.10:2870/dmr.xml` |
| `9741/tcp` | `192.168.1.176` -> `192.168.0.10:9741` (+ OpenWrt DNAT `wan->lan`) | `LibSPKConnect.dll` / `MessageConvert.dll` | AES-256-CBC encrypted JSON command/status channel (`PRODUCT_INFO`, `PLAY_INFO_REQ`, etc.) |
| `9742/udp` | LAN broadcast / `wan->lan` DNAT | `LibSPKConnect.dll` | Legacy Multiroom UDP broadcast discovery |
| `20000/udp` | `0.0.0.0:20000` | `MusicFlow.exe` | Internal/speaker UDP listener observed in runtime socket table |
| `49252/tcp` | `0.0.0.0:49252` (`wan->lan` DNAT) | `MusicFlowDMS.exe` (`DMCStack.dll`) | DLNA/UPnP HTTP media server for local PC audio streaming |

### 2.3 Exact OpenWrt Router Configuration (`/etc/` on `192.168.1.1`)

Backups created before edits:
- `/etc/config/firewall.bak-20261117` (`2,974` bytes)
- `/etc/smcroute.conf.bak-20261117` (`2,783` bytes)

Packages installed via `apk`:
- `smcroute` (`2.5.6-r1`, service enabled via `/etc/init.d/smcroute`, daemon `/usr/sbin/smcrouted`)

#### A. `/etc/smcroute.conf`
```conf
phyint br-lan enable ttl-threshold 1
phyint wan enable ttl-threshold 1

mgroup from br-lan group 239.255.255.250
mgroup from wan group 239.255.255.250

mroute from br-lan group 239.255.255.250 to wan
mroute from wan group 239.255.255.250 to br-lan
```
- Verified active MFC entry via `/proc/net/ip_mr_cache` and `smcroutectl show`: multicast packets flow bidirectionally between `br-lan` (`iif 6`) and `wan` (`iif 1`).

#### B. `/etc/firewall.ssdp-ttl.sh` (mangle TTL bump, persisted via UCI `firewall.@include[0]`)
```sh
iptables -t mangle -C PREROUTING -d 239.255.255.250 -p udp --dport 1900 -j TTL --ttl-set 4 2>/dev/null || \
    iptables -t mangle -A PREROUTING -d 239.255.255.250 -p udp --dport 1900 -j TTL --ttl-set 4
```
- **Why this is required:** SSDP packets normally leave hosts with `IP TTL=1`, causing Linux kernel multicast routing to decrement TTL to `0` and drop them before crossing interfaces. Rewriting TTL to `4` in `mangle PREROUTING` allows `smcrouted` to forward packets across `br-lan <-> wan`. (Initially tried `/etc/nftables.d/20-ssdp-ttl.nft` with `ip ttl set 4`, which `fw4` rejected with `Chain of type "filter" is not supported`; removed that `.nft` file and used `iptables -t mangle` via `firewall.@include` cleanly.)

#### C. `/etc/nftables.d/20-ssdp-reply.nft`
```nft
chain dstnat_wan {
	ip saddr 192.168.0.0/24 ip daddr 192.168.0.4 udp sport { 1900, 2870 } udp dport 1024-65535 dnat ip to 192.168.1.176
}
```
- **Why this is required:** Replaces earlier coarse UCI range redirects (`MusicFlow-SSDP-Reply-2870` / `MusicFlow-SSDP-Reply-1900`), preserving the exact destination UDP port (`1024-65535`) of SSDP `200 OK` unicast replies sent from the SJ8 (`192.168.0.10:2870` or `1900`) to `192.168.0.4`.

#### D. UCI Firewall Redirects, Rules & Includes (`uci show firewall`)
```ini
firewall.@redirect[2]=redirect
firewall.@redirect[2].name='MusicFlow-SSDP'
firewall.@redirect[2].target='DNAT'
firewall.@redirect[2].src='wan'
firewall.@redirect[2].src_ip='192.168.0.0/24'
firewall.@redirect[2].src_dport='1900'
firewall.@redirect[2].dest='lan'
firewall.@redirect[2].dest_ip='192.168.1.176'
firewall.@redirect[2].dest_port='1900'
firewall.@redirect[2].proto='udp'
firewall.@redirect[2].family='ipv4'
firewall.@redirect[2].src_dip='192.168.0.4'

firewall.@redirect[3]=redirect
firewall.@redirect[3].name='MusicFlow-Control'
firewall.@redirect[3].target='DNAT'
firewall.@redirect[3].src='wan'
firewall.@redirect[3].src_ip='192.168.0.0/24'
firewall.@redirect[3].src_dport='9741'
firewall.@redirect[3].dest='lan'
firewall.@redirect[3].dest_ip='192.168.1.176'
firewall.@redirect[3].dest_port='9741'
firewall.@redirect[3].proto='tcp'
firewall.@redirect[3].family='ipv4'

firewall.@redirect[4]=redirect
firewall.@redirect[4].name='MusicFlow-Discovery'
firewall.@redirect[4].target='DNAT'
firewall.@redirect[4].src='wan'
firewall.@redirect[4].src_ip='192.168.0.0/24'
firewall.@redirect[4].src_dport='9742'
firewall.@redirect[4].dest='lan'
firewall.@redirect[4].dest_ip='192.168.1.176'
firewall.@redirect[4].dest_port='9742'
firewall.@redirect[4].proto='udp'
firewall.@redirect[4].family='ipv4'

firewall.@redirect[5]=redirect
firewall.@redirect[5].name='MusicFlow-DMS'
firewall.@redirect[5].target='DNAT'
firewall.@redirect[5].src='wan'
firewall.@redirect[5].src_ip='192.168.0.0/24'
firewall.@redirect[5].src_dport='49252'
firewall.@redirect[5].dest='lan'
firewall.@redirect[5].dest_ip='192.168.1.176'
firewall.@redirect[5].dest_port='49252'
firewall.@redirect[5].proto='tcp'
firewall.@redirect[5].family='ipv4'

firewall.@rule[9]=rule
firewall.@rule[9].name='Allow-HB710-MusicFlow-Fwd'
firewall.@rule[9].src='wan'
firewall.@rule[9].src_ip='192.168.0.0/24'
firewall.@rule[9].dest='lan'
firewall.@rule[9].dest_ip='192.168.1.176'
firewall.@rule[9].proto='tcp' 'udp'
firewall.@rule[9].dest_port='1900 9741 9742 49252'
firewall.@rule[9].family='ipv4'
firewall.@rule[9].target='ACCEPT'

firewall.@rule[10]=rule
firewall.@rule[10].name='Allow-SSDP-Multicast-In'
firewall.@rule[10].src='wan'
firewall.@rule[10].src_ip='192.168.0.0/24'
firewall.@rule[10].dest_ip='239.255.255.250'
firewall.@rule[10].proto='udp'
firewall.@rule[10].dest_port='1900'
firewall.@rule[10].family='ipv4'
firewall.@rule[10].target='ACCEPT'

firewall.@rule[11]=rule
firewall.@rule[11].name='Allow-SSDP-Multicast-Fwd'
firewall.@rule[11].src='wan'
firewall.@rule[11].src_ip='192.168.0.0/24'
firewall.@rule[11].dest='lan'
firewall.@rule[11].dest_ip='239.255.255.250'
firewall.@rule[11].proto='udp'
firewall.@rule[11].dest_port='1900'
firewall.@rule[11].family='ipv4'
firewall.@rule[11].target='ACCEPT'

firewall.@include[0]=include
firewall.@include[0].type='script'
firewall.@include[0].path='/etc/firewall.ssdp-ttl.sh'
firewall.@include[0].fw4_compatible='1'
```

---

## 3. Wine Prefix Architecture, Process Interaction & Application Patches (`cachyos-esketids`)

- **Prefix Location:** `/home/esketids/.local/share/wineprefixes/musicflow` (Wine `wine-11.22` WoW64, Windows 10 target `10.0.19045.0`)
- **Runtimes Installed via `winetricks`:** `vcrun2010`, `dotnet40`, `corefonts`, `tahoma`

### 3.1 How the Wine Processes Interact on CachyOS

1. **`wineserver`** (e.g. PID `33552`):
   - Acts as the shared Windows kernel/object/registry/socket broker for all processes in `~/.local/share/wineprefixes/musicflow`.
   - Maps Win32 registry reads/writes to `~/.local/share/wineprefixes/musicflow/{system.reg,user.reg,userdef.reg}`, coordinates inter-process window messages (`FindWindow` / `SendMessage` between `MusicFlow.exe`, `MusicFlowTray.exe`, and `MusicFlowDMS.exe`), and translates Winsock (`ws2_32.dll`) calls into Linux POSIX sockets on `enp0s31f6` (`192.168.1.176`).
2. **`MusicFlow.exe`** (e.g. PID `33623`, 32-bit `.NET 4.0` WPF application):
   - Entrypoint GUI (`MainWindow`, X11 window `MusicFlow_esketids` `0x3a0000d`, `980x655`).
   - Startup sequence observed in `261117_064325.txt`:
     1) Reads `HKLM\Software\Wow6432Node\LGE\Music Flow\InstallPath` and `HKCU\Software\LGE\Music Flow\Parameters` (`Version`, `Filedate`).
     2) Checks `MFOption.ini` (`[OPTION] Log=TRUE`) and opens diagnostic log under `ProgramData\LG Software\Music Flow\Log\Main\esketids\`.
     3) Performs Windows Firewall check (`LibGlobal.dll` -> `HNetCfg.FwPolicy2` / `HNetCfg.FWRule` for `1900`, `MUSIC FLOW DMS`, etc.), triggering one stubbed `MessagePopupView` modal in Wine (dismissing OK/Cancel proceeds normally).
     4) Launches `MusicFlowTray.exe start` and checks for software updates (`52.78.8.117:80`, `124.155.222.112:80`).
     5) Starts `SPK Discovery` & `DiscoveryTimer` (5-second periodic timer) using `LibSPKConnect.dll` (`MSearchTCPClient`, `MSearchSubFunction`), binding `0.0.0.0:20000/udp` and an ephemeral SSDP socket, sending `M-SEARCH` to `239.255.255.250:1900`.
     6) Upon receiving the SJ8 `200 OK` from `192.168.0.10:2870`, opens persistent TCP socket `192.168.1.176:42416 -> 192.168.0.10:9741` and runs the JSON control handshake.
     7) Attempts `NativeWifi.WlanClient()` (`SHARE_HOME_SSID`) — catches `Win32Exception (0x80004005): Call not implemented` cleanly because `cachyos-esketids` has no Wi-Fi NIC.
3. **`MusicFlowTray.exe start`** (e.g. PID `33709`, X11 window `MusicFlowTray_esketids` `0x4200006`):
   - Manages background schedule XML (`Common\MusicFlow_Schedule.xml`), launches/monitors `MusicFlowDMS.exe -autoservice`, and relays `DMS -> APP` messages (`DMSRES_ADDFOLDER_DONE`).
4. **`MusicFlowDMS.exe -autoservice`** (e.g. PID `33747`, located in `DMS\`):
   - Native 32-bit C++/MFC DLNA Digital Media Server (`DMCStack.dll` / `LibDmsData.dll`).
   - Uses registered 32-bit DirectShow filters from `MFFilter\` (`*.ax`, `VSFilter.dll`) and helper codecs (`av_dll.dll`, `av_proxy.dll`).
   - Listens on `0.0.0.0:1900/udp` (SSDP announcements) and `0.0.0.0:49252/tcp` (HTTP media streaming), logging to `ProgramData\LG Software\Music Flow\Log\DMS\dms_log.txt`.

### 3.2 Installed Prefix Files, Registry & Launcher Assets

- **Extracted & Installed Directories:**
  - `MusicFlowComponent/*` -> `/home/esketids/.local/share/wineprefixes/musicflow/drive_c/Program Files (x86)/LG Software/Music Flow/`
  - `MFFILTER/*` -> `.../LG Software/Music Flow/MFFilter/` (all `.ax` DirectShow filters and `VSFilter.dll` registered with `syswow64\regsvr32.exe /s`)
  - `SYSTEM/av_dll.dll`, `SYSTEM/av_proxy.dll` -> `.../drive_c/windows/syswow64/`
  - `Common/DLL/64bit/av_dll.dll`, `av_proxy.dll` -> `.../drive_c/windows/system32/`
  - Cleanups performed on CachyOS: removed temporary `/tmp/mf-extract`, `/tmp/unshield*`, `/tmp/test_*`, `/tmp/dump_*`, `/tmp/click_ok*`, `/tmp/check_*`, `/tmp/sig*`, `/tmp/mf-*.png`, `/tmp/mf-*.log`.
- **Wine Registry Modifications Verified:**
  - `HKCU\Software\Microsoft\Avalon.Graphics` -> `DisableHWAcceleration = REG_DWORD 0x1` (fixes D3D9/OpenGL WPF black window on `MainWindow`)
  - `HKCU\Software\Wine\Direct3D` -> `DirectDrawRenderer = REG_SZ gdi`
  - `HKLM\Software\Microsoft\Windows NT\CurrentVersion\FontSubstitutes` -> `Malgun Gothic = Arial`, `Segoe UI = Tahoma` (plus `corefonts` + `tahoma`, fixing `FontFamily.get_FirstFontFamily()` `FailFast` crash)
  - `HKLM\Software\Wow6432Node\LGE\Music Flow` -> `InstallPath = C:\Program Files (x86)\LG Software\Music Flow`
  - `HKCU\Software\LGE\Music Flow\Parameters` -> `Version = 1.1.1611.701`, `Filedate = 201611071407` (and `ProtocolVersion = 1`)
- **Prefix Configuration Files:**
  - Wine `hosts` (`/home/esketids/.local/share/wineprefixes/musicflow/drive_c/windows/system32/drivers/etc/hosts`):
    ```text
    127.0.0.1 localhost
    192.168.1.176 cachyos-esketids
    ```
  - Runtime diagnostic logger (`.../LG Software/Music Flow/MFOption.ini`):
    ```ini
    [OPTION]
    Log=TRUE
    ```
- **Desktop Integration & Launcher:**
  - Wrapper script: `~/.local/bin/musicflow` (auto-exports `WINEPREFIX`, `DISPLAY=:0`, `XAUTHORITY` from `/run/user/$(id -u)/xauth_*`, `cd`s into install folder, `exec wine "$MF_DIR/MusicFlow.exe" "$@"`)
  - Application icon: `~/.local/share/icons/hicolor/256x256/apps/musicflow.png` (`256x256` extracted from `MusicFlow.ico`)
  - Desktop shortcuts: `~/.local/share/applications/musicflow.desktop` and `~/Desktop/Music Flow.desktop` (`StartupWMClass=musicflow.exe`)

### 3.3 Discovery Root Causes & `LibSPKConnect.dll` Binary Patch

Even after OpenWrt relayed SSDP `200 OK` packets (`2,878` bytes confirmed in `nf_conntrack`) to `MusicFlow.exe`, `Connected Speakers` only showed `esketids` due to **two Wine/.NET bugs** in `LibSPKConnect.dll`:

1. **Bug 1 (`127.0.1.1` Subnet Mask Crash):**
   - `MSearchSubFunction.GetLocalIP()` calls `Dns.GetHostEntry(Dns.GetHostName())`, which on CachyOS `/etc/hosts` returned `127.0.1.1`.
   - `GetSubnetMask("127.0.1.1")` then threw `ArgumentException: Can't find subnetmask for IP address '127.0.1.1'`, aborting `Discovery()`.
   - **Fix:** Added `192.168.1.176 cachyos-esketids` to the Wine prefix's `etc\hosts`, causing `GetLocalIP()` to resolve `192.168.1.176` (`255.255.255.0`).
2. **Bug 2 (`IP_MULTICAST_IF` `WSAEINVAL` in Wine `ws2_32`):**
   - Isolated via custom Wine test binary (`/tmp/test_mcast.cs`) + IL inspection of `LibSPKConnect.MSearchTCPClient.Discovery(Boolean)`.
   - `Discovery(Boolean)` calls `GetBestInterface` (`rc=0, idx=2`) and passes `IPAddress.HostToNetworkOrder(idx)` (`0.0.0.2`) to `Socket.SetSocketOption(SocketOptionLevel.IP, SocketOptionName.MulticastInterface, int)`.
   - Wine 11.22 rejects an interface index integer on `IP_MULTICAST_IF` with `SocketException (0x80004005): Invalid argument` (only `ipbytes` or omitting the option succeeds).
   - **Fix:** Backed up `LibSPKConnect.dll` -> `LibSPKConnect.dll.orig` (`33,272` bytes), and replaced the exact 20-byte `SetSocketOption(MulticastInterface)` IL call sequence inside `Discovery(Boolean)`:
     - Original bytes: `02 7b 02 00 00 04 16 1f 09 09 28 2c 00 00 0a 6f 2b 00 00 0a`
     - Patched bytes: 20 x `0x00` (`nop`), letting Linux route multicast `M-SEARCH` via the default `239.255.255.250` interface (`enp0s31f6`).

---

## 4. LG SJ8 Wire Protocol & Mac Wi-Fi Provisioning (`sj8-wifi-setup.py`)

Extracted from `MessageConvert.dll` and `LibSPKConnect.dll`:
- **Framing on TCP `9741`:** `0x10` + 4-byte big-endian ciphertext length + AES-256-CBC (PKCS#7) ciphertext
- **AES-256-CBC Key:** `4efgvbn m546Uy7kolKrftgbn =-0u&~` (32 bytes)
- **AES-256-CBC IV:** `54eRty@hkL,;/y9U` (16 bytes)
- **Reusable Mac script:** `/Users/esketids/AI_RELATED/DEEPSEEK/CACHYOS/musicflow/scripts/sj8-wifi-setup.py` (`chmod +x`, verified with `py_compile`; uses `openssl enc -aes-256-cbc`).
- **Provisioning steps executed from MacBook (`cachyos-esketids` has no Wi-Fi/BT hardware):**
  1. Connected Mac `en0` to soundbar AP `MusicFlow_Setup` (`192.168.5.101 -> 192.168.5.100:9741`). (macOS `networksetup -setairportnetwork` fails without Location Services entitlement, so macOS GUI selection / `CoreWLAN` was used.)
  2. Sent `SHARE_HOME_INFO` (`{"data":{"pwd":"<password>","ssid":"o"},"msg":"SHARE_HOME_INFO"}`) -> `{"msg":"SHARE_HOME_INFO","result":"OK"}`.
  3. SJ8 rebooted/joined HB710 Wi-Fi `o` at **`192.168.0.10`** (`icmp` ~`5.1 ms`, Google Cast `8008`/`8009`, UPnP `2870/dmr.xml`, Control `9741/tcp`).
  4. Sent `SPK_ADD_SET` (`"LG SJ8"`) -> `OK`; `PRODUCT_INFO` returned `reg: true`, `petname: "Sound Bar SJ8"`, `modelname: "SJ8"`, `modelnum: 14`, `wirelessmac: "7c:1c:4e:3b:b0:22"`, `btmac: "08:EF:3B:9C:2D:4F"`, `bever: "NB9.504.00629.C"`.

---

## 5. Final Verification & Observed Runtime State

- **Running Processes (`cachyos-esketids`):**
  - `33552` `/usr/lib/wine/../../bin/wineserver`
  - `33623` `C:\Program Files (x86)\LG Software\Music Flow\MusicFlow.exe`
  - `33709` `C:\Program Files (x86)\LG Software\Music Flow\MusicFlowTray.exe start`
  - `33747` `C:\Program Files (x86)\LG Software\Music Flow\DMS\MusicFlowDMS.exe -autoservice`
- **Verified Sockets (`ss -tupn` / `ss -tulpn`):**
  - `tcp ESTAB 192.168.1.176:42416 -> 192.168.0.10:9741` (`MusicFlow.exe`, PID `33623`)
  - `udp 0.0.0.0:20000` (`MusicFlow.exe`, PID `33623`) & `udp 0.0.0.0:43577` (`MusicFlow.exe`, PID `33623`)
  - `udp 0.0.0.0:1900` (`MusicFlowDMS.exe`, PID `33747`)
  - `tcp LISTEN 0.0.0.0:49252` (`MusicFlowDMS.exe` / `wineserver`)
- **Verified Live Application Log (`.../Log/Main/esketids/261117_064325.txt`, background logging active via `MFOption.ini`):**
  - `APP -> SPK TCP(192.168.0.10) : 192.168.0.10:9741에 연결 완료`
  - Successful exchanges: `PRODUCT_INFO`, `SYSTEM_VER_REQ`, `PLAY_INFO_REQ`, `FUNC_INFO_REQ`, `ALARM_STATE_NOTI`, `SETTING_INFO_REQ` (`"ipv4addr": "192.168.0.10"`), `FUNC_NAME_REQ` (`"func": 15, "name": "Optical/HDMI ARC"`), `AUTO_POWER_SET` (`OK`).
- **Visual Confirmation (`mf-win-paired.png`):**
  - `Connected Speakers` pane displays `esketids` and `LG SJ8` with active group, link, input-source selector, and playback controls.

---

## 6. Online Research Findings, Remaining Caveats & Rollback Notes

- **Online cross-subnet guidance (`search_web`):** LG Music Flow across subnets requires SSDP multicast relaying (`239.255.255.250:1900/udp`), unicast `9741/tcp` access, and stable DHCP reservations (standard static routes alone never forward multicast discovery).
- **HB710 Static Route for PC DLNA (`MusicFlowDMS.exe`) Streaming:**
  - App control (`9741/tcp`) and Spotify Connect work as-is through OpenWrt masquerade.
  - If you stream local PC files from `MusicFlowDMS.exe` (`192.168.1.176:49252`) to the SJ8 (`192.168.0.10`), add a **Static Route** in HB710 (`Network -> Routing -> Static Routing`): Destination `192.168.1.0`, Mask `255.255.255.0`, Gateway `192.168.0.4`. Also reserve `192.168.0.10` for `7c:1c:4e:3b:b0:22` on HB710.
- **Cold-start modal:** On fresh app startup, Wine displays the one-time unimplemented `HNetCfg.FwPolicy2` firewall modal; click OK or Cancel to enter `MainWindow`.
- **Disable runtime app logging later (optional):** Edit `MFOption.ini` (`Log=FALSE`) if you don't want ongoing daily log growth under `ProgramData\LG Software\Music Flow\Log\Main\esketids\`.
- **Rollback options:**
  - Restore original DLL: `cp LibSPKConnect.dll.orig LibSPKConnect.dll`
  - Restore OpenWrt firewall/smcroute: copy `/etc/config/firewall.bak-20261117` & `/etc/smcroute.conf.bak-20261117` back, remove `/etc/nftables.d/20-ssdp-reply.nft` and `/etc/firewall.ssdp-ttl.sh`, and run `/etc/init.d/firewall restart && /etc/init.d/smcroute restart`.

---

## 7. Alarm Function Verification (`192.168.0.10:9741`)

- **Firmware support:** `SETTING_INFO_REQ` reports `"visible_alarm": true`; `MusicFlow.exe` exposes Alarm scheduling in `SettingViewModel` / `SettingView` using `MSGConvert.SendCommand.ALARM_SET` (`id`, `mode`, `hour`, `minute`, `day`, `shuffle`, `duration`, `volume`, `type`, `songpath`, `enable`, `title`, `ipaddress`, `modifiedid`), `SET_ALARM_PLAYLIST`, `ALARM_LIST_REQ`, and `ALARM_STATE_REQ` (`ALARM_STATE_NOTI`).
- **Live protocol verification (`192.168.0.10:9741` AES-256-CBC):**
  - Initial `ALARM_LIST_REQ` -> `{'msg': 'ALARM_LIST_REQ', 'data': {'info': []}, 'result': 'OK'}` and `ALARM_STATE_REQ` -> `{'msg': 'ALARM_STATE_NOTI', 'data': {'on': False}}`.
  - **Add (`mode: 0`):** Created test alarm (`23:59`, `day: 1`, `duration: 15`, `volume: 10`, `type: 0`, `enable: true`) -> `{'msg': 'ALARM_SET', 'result': 'OK', 'data': {'id': 0}}`; `ALARM_LIST_REQ` confirmed it persisted on the SJ8 (`id: 0`, `hour: 23`, `minute: 59`, `enable: True`).
  - **Disable / Enable (`mode: 4` / `mode: 3`):** Disabled (`mode: 4`) -> `OK` (`enable: False` confirmed in `ALARM_LIST_REQ`); re-enabled (`mode: 3`) -> `OK` (`enable: True` confirmed in `ALARM_LIST_REQ`).
  - **Delete (`mode: 2`):** Deleted `id: 0` (`mode: 2`) -> `OK`; final `ALARM_LIST_REQ` returned `info: []` (clean state restored without triggering an audible ring).
- **Important note on playlist alarms vs. chime alarms:** Built-in tone/chime alarms (`type: 0`, `songpath: ""`) are stored and managed directly on the SJ8 itself. If you set a **local PC music playlist** as the alarm sound (`SET_ALARM_PLAYLIST` referencing `MusicFlowDMS.exe` at `192.168.1.176:49252`), the SJ8 also needs the HB710 Static Route (`192.168.1.0/24 -> 192.168.0.4`) so it can pull the audio file from CachyOS when the alarm triggers.
