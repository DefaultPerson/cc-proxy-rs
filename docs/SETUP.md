# claude-code-proxy — Setup with OpenClaw

HTTP proxy that wraps Claude Code CLI as a subprocess, exposing it as Anthropic Messages API.

## Prerequisites

- Claude Code CLI installed and authenticated: `npm i -g @anthropic-ai/claude-code && claude auth login`
- Active Claude subscription (Max, Team, or Enterprise)

## Install

### From release (recommended)

Download the latest binary from [Releases](https://github.com/DefaultPerson/claude-code-proxy-rs/releases/latest):

```bash
# Example for Linux x86_64
curl -L https://github.com/DefaultPerson/claude-code-proxy-rs/releases/latest/download/claude-code-proxy -o ~/.local/bin/claude-code-proxy
chmod +x ~/.local/bin/claude-code-proxy
```

### From source

```bash
cargo build --release
cp target/release/claude-code-proxy ~/.local/bin/
```

## Run

```bash
claude-code-proxy --port 3456 --cwd ~ --embed-system-prompt
```

Flags:
- `--port` — listen port (default 3456)
- `--cwd` — working directory for CLI subprocess (default `.`)
- `--embed-system-prompt` — **recommended for OpenClaw**: embeds system prompt in text with `<system>` tags, keeps Claude Code's default system prompt intact
- `--replace-system-prompt` — alternative: replaces Claude Code's system prompt entirely via `--system-prompt` CLI flag
- `--effort` — thinking effort: `low`, `medium`, `high`, `max`

## Systemd (optional)

```bash
cat > ~/.config/systemd/user/claude-code-proxy.service << 'EOF'
[Unit]
Description=Claude Code Proxy
After=network.target

[Service]
ExecStart=%h/.local/bin/claude-code-proxy --port 3456 --cwd %h --embed-system-prompt
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=claude_code_proxy=info

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now claude-code-proxy
```

## OpenClaw config

In `~/.openclaw/openclaw.json`:

```json
{
  "models": {
    "providers": {
      "claude-proxy": {
        "baseUrl": "http://localhost:3456",
        "apiKey": "not-needed",
        "api": "anthropic-messages",
        "models": [
          { "id": "claude-sonnet-4-6", "name": "Claude Sonnet 4.6" },
          { "id": "claude-opus-4-6", "name": "Claude Opus 4.6" },
          { "id": "claude-haiku-4-5", "name": "Claude Haiku 4.5" }
        ]
      }
    }
  },
  "agents": {
    "defaults": {
      "model": {
        "primary": "claude-proxy/claude-sonnet-4-6"
      }
    }
  }
}
```

Key points:
- `api`: must be `anthropic-messages` (not `openai-completions`)
- `baseUrl`: `http://localhost:3456` — without `/v1` (SDK adds `/v1/messages` automatically)
- `apiKey`: any non-empty string (proxy ignores it)
- Model IDs must match exactly (with `-6`/`-5` suffixes)

After editing config:
```bash
openclaw gateway restart
```

## Verify

```bash
curl -sN http://localhost:3456/v1/messages \
  -H 'content-type: application/json' \
  -d '{"model":"claude-sonnet-4-6","max_tokens":50,"messages":[{"role":"user","content":"Say hi"}],"stream":true}'
```

Expected: SSE stream with `message_start` → `content_block_delta` → `message_stop`.

## Architecture notes

- Each request spawns a `claude` CLI subprocess (`-p --output-format stream-json`)
- CLI does multi-turn tool calls (Read, Bash, etc.) internally
- Proxy filters SSE: only text content blocks are forwarded (thinking/tool_use/signature blocks are stripped to avoid SDK parsing issues)
- `result.result` from CLI used as fallback when no text was streamed
- Stateless: no session persistence, full conversation history sent each request
