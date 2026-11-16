#!/usr/bin/env python3
"""Provision an LG Music Flow / SJ8 soundbar while connected to MusicFlow_Setup."""

import argparse
import getpass
import json
import socket
import struct
import subprocess
import sys

AES_KEY = b"4efgvbn m546Uy7kolKrftgbn =-0u&~".hex()
AES_IV = b"54eRty@hkL,;/y9U".hex()
SJ8_IP = "192.168.5.100"
SJ8_PORT = 9741


def encrypt_packet(payload: dict) -> bytes:
    raw = json.dumps(payload, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    enc = subprocess.run(
        ["openssl", "enc", "-aes-256-cbc", "-K", AES_KEY, "-iv", AES_IV, "-nosalt"],
        input=raw,
        capture_output=True,
        check=True,
    ).stdout
    return b"\x10" + len(enc).to_bytes(4, "big") + enc


def decrypt_response(sock: socket.socket) -> dict:
    hdr = sock.recv(5)
    if len(hdr) < 5:
        raise RuntimeError(f"Short header from SJ8: {hdr!r}")
    length = struct.unpack(">I", hdr[1:5])[0]
    body = bytearray()
    while len(body) < length:
        chunk = sock.recv(length - len(body))
        if not chunk:
            break
        body.extend(chunk)
    if hdr[0] == 0x10:
        dec = subprocess.run(
            ["openssl", "enc", "-d", "-aes-256-cbc", "-K", AES_KEY, "-iv", AES_IV, "-nosalt"],
            input=bytes(body),
            capture_output=True,
            check=True,
        ).stdout
        return json.loads(dec.decode("utf-8"))
    return json.loads(bytes(body).decode("utf-8"))


def send_cmd(ip: str, port: int, payload: dict) -> dict:
    with socket.create_connection((ip, port), timeout=5.0) as s:
        s.sendall(encrypt_packet(payload))
        return decrypt_response(s)


def main() -> int:
    parser = argparse.ArgumentParser(description="Push Wi-Fi credentials to LG SJ8 on MusicFlow_Setup")
    parser.add_argument("--ssid", default="o", help="Target HB710 Wi-Fi SSID (default: o)")
    parser.add_argument("--password", help="Wi-Fi password (omit to prompt securely)")
    parser.add_argument("--ip", default=SJ8_IP, help=f"Soundbar IP in setup mode (default: {SJ8_IP})")
    args = parser.parse_args()

    password = args.password if args.password is not None else getpass.getpass(f"Password for SSID '{args.ssid}': ")

    print(f"Querying {args.ip}:{SJ8_PORT}...")
    info = send_cmd(args.ip, SJ8_PORT, {"data": {"day": 1, "hour": 6, "min": 20, "option": 0}, "msg": "PRODUCT_INFO"})
    print("PRODUCT_INFO:", json.dumps(info))

    print(f"Sending SHARE_HOME_INFO for SSID '{args.ssid}'...")
    res = send_cmd(args.ip, SJ8_PORT, {"data": {"pwd": password, "ssid": args.ssid}, "msg": "SHARE_HOME_INFO"})
    print("SHARE_HOME_INFO:", json.dumps(res))
    return 0 if res.get("result") == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
