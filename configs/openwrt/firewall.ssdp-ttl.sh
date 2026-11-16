iptables -t mangle -C PREROUTING -d 239.255.255.250 -p udp --dport 1900 -j TTL --ttl-set 4 2>/dev/null || \
    iptables -t mangle -A PREROUTING -d 239.255.255.250 -p udp --dport 1900 -j TTL --ttl-set 4
