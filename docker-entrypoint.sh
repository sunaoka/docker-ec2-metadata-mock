#!/bin/sh

set -eu

metadata_ipv4_address='169.254.169.254'
metadata_ipv4_cidr="${metadata_ipv4_address}/32"
metadata_ipv6_address='fd00:ec2::254'
metadata_ipv6_cidr="${metadata_ipv6_address}/128"
nat_chain='EC2_METADATA_MOCK'

case "${IMDS_LISTEN_PORT}" in
'' | *[!0-9]*)
    echo 'IMDS_LISTEN_PORT must be an integer from 1 to 65535' >&2
    exit 2
    ;;
esac

if [ "${IMDS_LISTEN_PORT}" -lt 1 ] || [ "${IMDS_LISTEN_PORT}" -gt 65535 ]; then
    echo 'IMDS_LISTEN_PORT must be an integer from 1 to 65535' >&2
    exit 2
fi

if ! ip -o -4 addr show dev lo | grep -Fq "inet ${metadata_ipv4_address}/"; then
    ip addr add "$metadata_ipv4_cidr" dev lo
fi

iptables -t nat -N "$nat_chain" 2>/dev/null || :
if ! iptables -t nat -C OUTPUT -d "$metadata_ipv4_address" -p tcp --dport 80 -j "$nat_chain" 2>/dev/null; then
    iptables -t nat -I OUTPUT 1 -d "$metadata_ipv4_address" -p tcp --dport 80 -j "$nat_chain"
fi
iptables -t nat -F "$nat_chain"
iptables -t nat -A "$nat_chain" -p tcp -j REDIRECT --to-ports "$IMDS_LISTEN_PORT"

if [ "${IMDS_IPV6_ENABLED:-0}" = '1' ]; then
    if ! ip -o -6 addr show dev lo | grep -Fq "inet6 ${metadata_ipv6_address}/"; then
        ip -6 addr add "$metadata_ipv6_cidr" dev lo
    fi

    ip6tables -t nat -N "$nat_chain" 2>/dev/null || :
    if ! ip6tables -t nat -C OUTPUT -d "$metadata_ipv6_address" -p tcp --dport 80 -j "$nat_chain" 2>/dev/null; then
        ip6tables -t nat -I OUTPUT 1 -d "$metadata_ipv6_address" -p tcp --dport 80 -j "$nat_chain"
    fi
    ip6tables -t nat -F "$nat_chain"
    ip6tables -t nat -A "$nat_chain" -p tcp -j REDIRECT --to-ports "$IMDS_LISTEN_PORT"
fi

exec /usr/local/bin/ec2-metadata-mock
