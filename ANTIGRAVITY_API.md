# Antigravity IDE Language Server — Reverse Engineering Findings

## Overview

Antigravity IDE uses a local gRPC-Web language server (`language_server.exe`) for all model communication.
The language server is started by the IDE and exposes local HTTPS endpoints with HTTP/2 + ALPN.

## Architecture

```
Antigravity IDE (Electron)
  └── language_server.exe (Go binary, gRPC-Web server)
      ├── Port 13500: Chrome DevTools Protocol (CDP) — Electron's DevTools
      ├── Port 13501: Host Bridge (HTTP/1.1, REST-like, requires Bearer token)
      ├── Port 13502: Main gRPC-Web server (HTTPS, HTTP/2, ALPN h2)
      └── Port 13503: HTTP server (returns HTML SPA for all paths)
```

**Note:** Ports are dynamically assigned. The language server command line contains:
- `--host_bridge_url=http://127.0.0.1:{port}` (host bridge)
- `--host_bridge_token={token}` (bearer token for host bridge)
- `--csrf_token={uuid}` (CSRF token for gRPC-Web)

## gRPC-Web Protocol Details

### Connection
- **URL:** `https://127.0.0.1:{port}/{service.typeName}/{method.name}`
- **Protocol:** HTTP/2 with ALPN `h2`
- **TLS:** Self-signed certificate (must disable verification)

### Headers
```
Content-Type: application/grpc-web+proto  (binary protobuf)
  OR
Content-Type: application/grpc-web+json   (JSON protobuf)

x-codeium-csrf-token: {csrf_token}  (from window.__APP_CONFIG__.csrfToken)
x-grpc-web: 1
x-user-agent: CONNECT_ES_USER_AGENT
```

### gRPC-Web Framing
Request body: `1 byte flag (0x00) + 4 bytes big-endian length + protobuf message`
Response body: Same framing, can have multiple frames (for streaming)

### Service Definition
- **Service:** `exa.language_server_pb.LanguageServerService`
- **Proto file:** `third_party/jetski/language_server_pb/language_server.proto`
- **Package:** `exa.language_server_pb`

## Working API Calls

### 1. GetCapabilities (empty request)
```python
# Request: empty protobuf message
# Response: {"supportsHookResultProtoBytes": true}
```

### 2. GetAvailableModels (empty request)
Returns 32 models:
- **Google Gemini:** gemini-2.5-pro, gemini-2.5-flash, gemini-3-flash, gemini-3.1-pro-high, gemini-3.1-pro-low, gemini-3.1-flash-lite, gemini-3.5-flash-low, gemini-3.5-flash-extra-low, gemini-3.6-flash-high/medium/low, gemini-3.7-flash-high/medium/low, gemini-3.8-flash-high/medium/low, gemini-pro-agent, gemini-3-flash-agent, chat_20706, chat_23310
- **Anthropic Claude:** claude-opus-4-6-thinking, claude-sonnet-4-6
- **OpenAI:** gpt-oss-120b-medium
- **Tab models:** tab_flash_lite_preview, tab_jump_flash_lite_preview

Model info includes: maxTokens, modelProvider, apiProvider, quotaInfo, displayName, supportsImages, supportsCumulativeContext, etc.

### 3. Heartbeat (empty request)
```json
{"lastExtensionHeartbeat": "2026-09-04T07:05:25.776564300Z"}
```

### 4. GetServerConfiguration (empty request)
```json
{
  "config": {
    "sidecars": {"enabled": true},
    "standalone": true,
    "appDataDir": "antigravity",
    "antigravityHub": true,
    "maxNumTrackedWorkspaces": 10
  }
}
```

### 5. GetLoadCodeAssist (empty request)
Returns tier info: `free-tier` (Antigravity, Gemini-powered)

### 6. RetrieveUserQuotaSummary (empty request)
Returns quota groups:
- **Gemini Models:** weekly limit + 5h limit
- **Claude and GPT models:** weekly limit + 5h limit

### 7. HandleStreamingCommand (server-streaming)
**Status:** Partially working — grpc-status=0 with correct path format, but no response messages yet.

**Request fields (HandleStreamingCommandRequest):**
- Field 1: `metadata` (Metadata message)
- Field 2: `document` (Document message)
- Field 3: `edit_options` (EditorOptions)
- Field 4: `requested_model_id` (Model enum)
- Field 5: `experiment_config` (ExperimentConfig)
- Field 6: `selection_start_line` (int32)
- Field 7: `selection_end_line` (int32)
- Field 8: `command_text` (string)
- Field 9: `request_source` (CommandRequestSource enum)
- Field 10: `mentioned_scope` (repeated ContextScope)
- Field 12: `action_pointer` (ActionPointer)
- Field 13: `diff_type` (DiffType enum)
- Field 16: `terminal_command_data` (TerminalCommandData)

**Metadata message fields:**
- Field 1: `api_key` (string)
- Field 2: `api_key_name` (string)
- Field 3: `request_id` (string)
- Field 4: `session_id` (string)
- Field 5: `ide_name` (string)
- Field 6: `ide_version` (string)
- Field 7: `extension_name` (string)
- Field 8: `extension_version` (string)

**Document message fields (from codeium_common.proto):**
- Field 1: `absolute_path` (string) — MUST use forward slashes: `C:/projekt/test.py`
- Field 2: `cursor_position` (CursorPosition)
- Field 6: `text` (string)

**Enum values:**
- `CommandRequestSource`: UNSPECIFIED=0, DEFAULT=1, PLAN=7, SUPERCOMPLETE=10, FAST_APPLY=12, TERMINAL=13, TAB_JUMP=14, CASCADE_CHAT=16
- `DiffType`: UNSPECIFIED=0, DELETE=1, INSERT=2, UNCHANGED=3

**Key findings for HandleStreamingCommand:**
- `requestSource` must be `CASCADE_CHAT` (16) for chat
- `document.absolutePath` must use forward slashes (`C:/...`), NOT backslashes
- Binary protobuf format works; JSON format has field name issues for Document
- Without `requested_model_id`, server accepts request (grpc-status=0) but returns no messages
- Need to find correct Model enum integer values to get actual chat responses

## How to Discover Ports at Runtime

```powershell
# Get language server process info
Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'language_server.exe' } | Select-Object ProcessId, CommandLine

# Extract from command line:
# --host_bridge_url=http://127.0.0.1:{bridge_port}
# --host_bridge_token={token}
# --csrf_token={csrf_uuid}

# Get listening ports
$ls_pid = (Get-Process language_server).Id
Get-NetTCPConnection -State Listen | Where-Object { $_.OwningProcess -eq $ls_pid } | Select-Object LocalPort

# The gRPC-Web port is the one serving HTTPS (usually the highest port)
# The CDP port is on the Antigravity process itself
$ag_pid = (Get-Process Antigravity).Id
Get-NetTCPConnection -State Listen | Where-Object { $_.OwningProcess -eq $ag_pid -and $_.LocalPort -gt 5000 } | Select-Object LocalPort

# Get CSRF token from CDP
curl http://127.0.0.1:{cdp_port}/json  # Find page target
# Then evaluate: window.__APP_CONFIG__.csrfToken
```

## Python Client Reference

```python
import asyncio, ssl, json, struct
import h2.connection, h2.config, h2.events

async def grpc_web_call(port, csrf_token, service, method, proto_body, use_json=False):
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    ctx.set_alpn_protocols(['h2'])
    
    reader, writer = await asyncio.open_connection('127.0.0.1', port, ssl=ctx)
    config = h2.config.H2Configuration(client_side=True, header_encoding='utf-8')
    conn = h2.connection.H2Connection(config=config)
    conn.initiate_connection()
    writer.write(conn.data_to_send())
    data = await reader.read(4096)
    conn.receive_data(data)
    writer.write(conn.data_to_send())
    
    ct = 'application/grpc-web+json' if use_json else 'application/grpc-web+proto'
    body = b'\x00' + struct.pack('>I', len(proto_body)) + proto_body
    
    headers = [
        (':method', 'POST'),
        (':path', f'/{service}/{method}'),
        (':scheme', 'https'),
        (':authority', f'127.0.0.1:{port}'),
        ('content-type', ct),
        ('x-codeium-csrf-token', csrf_token),
        ('x-grpc-web', '1'),
        ('x-user-agent', 'CONNECT_ES_USER_AGENT'),
        ('content-length', str(len(body))),
    ]
    
    stream_id = conn.get_next_available_stream_id()
    conn.send_headers(stream_id, headers, end_stream=False)
    conn.send_data(stream_id, body, end_stream=True)
    writer.write(conn.data_to_send())
    
    all_data = b""
    response_headers = {}
    for _ in range(100):
        try:
            data = await asyncio.wait_for(reader.read(8192), timeout=60)
            if not data: break
            events = conn.receive_data(data)
            for event in events:
                if hasattr(event, 'stream_id') and event.stream_id == stream_id:
                    if isinstance(event, h2.events.ResponseReceived):
                        for k, v in event.headers:
                            k = k.decode() if isinstance(k, bytes) else k
                            v = v.decode() if isinstance(v, bytes) else v
                            response_headers[k] = v
                    elif isinstance(event, h2.events.DataReceived):
                        all_data += event.data
                        conn.acknowledge_received_data(event.flow_controlled_length, event.stream_id)
            writer.write(conn.data_to_send())
        except asyncio.TimeoutError:
            break
    
    writer.close()
    
    # Parse gRPC-Web frames
    messages = []
    offset = 0
    while offset + 5 <= len(all_data):
        flag = all_data[offset]
        msg_len = struct.unpack('>I', all_data[offset+1:offset+5])[0]
        messages.append(all_data[offset+5:offset+5+msg_len])
        offset += 5 + msg_len
    
    return response_headers, messages
```

## Next Steps for opencode-rs Integration

1. **Find Model enum integer values** — need to decode codeium_common.proto or intercept actual request
2. **Get HandleStreamingCommand to return messages** — likely need correct `requested_model_id`
3. **Implement as Rust provider** — using `h2` crate for HTTP/2 + gRPC-Web
4. **Dynamic port discovery** — scan for language_server.exe process and its listening ports
5. **CSRF token discovery** — use CDP or parse language_server.exe command line
