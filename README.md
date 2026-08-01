<div align="center">

# youtube-chat-rs

YouTube chat in the terminal.

<img src="assets/preview.png" />

</div>

A terminal UI (TUI) app for viewing YouTube live chat. Currently only supports reading live messages, with more interactive features planned.

Alongside regular messages, the chat view renders Super Chats and Super Stickers, memberships (new, milestone, gifted) and moderation notices (deleted messages, bans, chat ended).

> Avatar rendering currently works only in **Kitty** terminals. Other terminals will display chat without avatars.

## Install

### Homebrew

```bash
brew install efekrskl/tap/ytc
```

### First Run Requirements

- A Google Cloud project with the YouTube Data API enabled
- OAuth desktop app credentials downloaded as `client_secret.json`

## How to Use

Run the application and connect to a stream using one of the following:

```bash
ytc --video-id <VIDEO_ID>

or

ytc --channel-name <CHANNEL_NAME>
```

or during local development

```bash
cargo run -- --video-id <VIDEO_ID>
```

If this is your first time running the app, you will be prompted for auth.

The connection recovers on its own: if the network drops or YouTube closes the
stream, `ytc` reconnects with exponential backoff and resumes from where it left
off, so no messages are lost or repeated. The status bar shows
`Reconnecting (n)` while this is happening.

## Files and logs

Everything lives under `~/.youtube-chat-rs/` (created with `0700`):

| File | Purpose |
| --- | --- |
| `client_secret.json` | Your OAuth desktop credentials (`0600`) |
| `token_cache.json` | Cached/refreshed OAuth tokens (`0600`) |
| `ytc.log` | Application log |
| `avatars/` | Cached profile pictures for Kitty rendering |

Logs go to the file rather than stderr so they cannot scribble over the TUI:

```bash
RUST_LOG=debug ytc --video-id <VIDEO_ID>
tail -f ~/.youtube-chat-rs/ytc.log
```

## Development

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

> **Note:** `proto/stream_list.proto` describes YouTube's streaming live-chat
> gRPC service, which Google does not publish or support. It was reconstructed
> from the wire format and may change or disappear without notice.
