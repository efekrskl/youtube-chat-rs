<div align="center">

# youtube-chat-rs

YouTube chat in the terminal.

<img src="assets/preview.png" />

</div>

A terminal UI (TUI) app for viewing YouTube live chat. Currently only supports reading live messages, with more interactive features planned. 

> Avatar rendering currently works only in **Kitty** terminals. Other terminals will display chat without avatars.

## How to Use

1. Create a Google Cloud project and enable the YouTube Data API.

2. Download your client_secret.json (OAuth credentials).

3. Run the application and connect to a stream using one of the following:

```bash
ytc --video-id <VIDEO_ID>

or

ytc --channel-name <CHANNEL_NAME>
```