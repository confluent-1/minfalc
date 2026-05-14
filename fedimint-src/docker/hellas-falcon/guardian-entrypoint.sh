#!/bin/bash
set -euo pipefail

# Guardian entrypoint: runs DKG on first start, then starts fedimintd.
#
# Environment variables expected:
#   FM_PEER_ID          — 0,1,2,3 (guardian index)
#   FM_FEDERATION_NAME  — federation name (default: hellas-falcon-bench)
#   FM_PEERS            — comma-separated peer API URLs
#   FM_DATA_DIR         — data directory (default: /data)
#   FM_BIND_P2P         — bind address for p2p (default: 0.0.0.0:8173)
#   FM_BIND_API         — bind address for API (default: 0.0.0.0:8174)

FM_DATA_DIR="${FM_DATA_DIR:-/data}"
FM_PEER_ID="${FM_PEER_ID:-0}"
FM_FEDERATION_NAME="${FM_FEDERATION_NAME:-hellas-falcon-bench}"
FM_BIND_P2P="${FM_BIND_P2P:-0.0.0.0:8173}"
FM_BIND_API="${FM_BIND_API:-0.0.0.0:8174}"

CONFIG_DIR="${FM_DATA_DIR}/config"
mkdir -p "${CONFIG_DIR}"

exec fedimintd \
    --data-dir "${FM_DATA_DIR}" \
    --bind-p2p "${FM_BIND_P2P}" \
    --bind-api "${FM_BIND_API}"
