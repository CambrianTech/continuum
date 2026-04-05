#!/bin/sh
# Continuum TLS Proxy — requires Tailscale certs.
#
# Provision certs (one time, from the machine running Tailscale):
#   tailscale cert --cert-file ~/.continuum/<hostname>.crt --key-file ~/.continuum/<hostname>.key <hostname>.ts.net
#
# On WSL2 (Tailscale on Windows side):
#   From PowerShell: tailscale cert bigmama.<tailnet>.ts.net
#   Then: cp /mnt/c/Users/<user>/bigmama.*.crt ~/.continuum/
#         cp /mnt/c/Users/<user>/bigmama.*.key ~/.continuum/

CRT=$(find /certs -maxdepth 1 -name "*.crt" 2>/dev/null | head -1)
KEY=$(find /certs -maxdepth 1 -name "*.key" 2>/dev/null | head -1)

if [ -z "$CRT" ] || [ -z "$KEY" ]; then
  echo ""
  echo "ERROR: No TLS certs found in ~/.continuum/"
  echo ""
  echo "Tailscale certs are required. Provision them once:"
  echo ""
  echo "  From the machine running Tailscale (Windows PowerShell for WSL2):"
  echo "    tailscale cert <hostname>.<tailnet>.ts.net"
  echo ""
  echo "  Then copy to ~/.continuum/:"
  echo "    cp <hostname>.<tailnet>.ts.net.crt ~/.continuum/"
  echo "    cp <hostname>.<tailnet>.ts.net.key ~/.continuum/"
  echo ""
  echo "  Then restart: docker compose restart tls-proxy"
  echo ""
  exit 1
fi

DOMAIN=$(basename "$CRT" .crt)
echo "TLS proxy: $DOMAIN"
echo "  :443  → widget-server:9003 (HTTPS)"
echo "  :9001 → node-server:9001   (WSS)"
echo "  :7443 → livekit:7880       (WSS)"

cat > /etc/caddy/Caddyfile << EOF
${DOMAIN} {
	tls ${CRT} ${KEY}
	reverse_proxy widget-server:9003
}

${DOMAIN}:9001 {
	tls ${CRT} ${KEY}
	reverse_proxy node-server:9001
}

${DOMAIN}:7443 {
	tls ${CRT} ${KEY}
	reverse_proxy livekit:7880
}
EOF

exec caddy run --config /etc/caddy/Caddyfile --adapter caddyfile
