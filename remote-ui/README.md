# OpenDeck Remote

A phone-friendly web app for configuring OpenDeck from another device on the same network. It is built for decks that run OpenDeck headless, such as a Raspberry Pi inside a wireless remote, but works with any OpenDeck install.

OpenDeck serves the app itself, so there is nothing to install on the phone: open the pairing link, then use "Add to Home Screen" to get an app icon.

## Turning it on

The remote server is off by default. Turn it on either way:

- Start OpenDeck with `--remote` (for example in the systemd unit or autostart entry on the Pi), or
- Set `"enabled": true` in `remote.json` in the OpenDeck config folder (`~/.config/opendeck/remote.json` on Linux).

`remote.json` is created on first start:

```json
{
	"enabled": false,
	"bind": "0.0.0.0",
	"port": 57200,
	"token": "<random pairing token>"
}
```

When the server starts, OpenDeck logs a pairing link such as `http://raspberrypi.local:57200/#token=...`. Open it on the phone, or open `http://<deck>:57200/` and paste the token. To unpair every phone, delete the `token` line and restart OpenDeck; a new token is generated.

The token is the only protection, and the server speaks plain HTTP, so only enable it on a network you trust.

## What the app can do

- Show the deck as it really is: the key images the deck displays, the dials, and the custom layout set in the desktop app
- Add, change, move, copy and remove actions on keys and dials, including the actions inside multi and toggle actions
- Edit key titles, test-press a key, and mark a slot as shown on every page
- Show plugin settings (property inspectors), so plugins such as Home Assistant can be set up from the phone
- Switch, add, rename, duplicate, reorder and delete pages
- Change brightness and the sleep timeout, and reload plugins

## Remote API

The app uses a small API that other tools, such as a plugin that generates pages, can use too. Send the token as `Authorization: Bearer <token>`, an `od_remote_token` cookie, or a `token` query parameter.

| Route             | What it does                                                                                                |
| ----------------- | ----------------------------------------------------------------------------------------------------------- |
| `GET /api/info`   | Name, version, API version, and whether the request is authorised. No token needed.                         |
| `POST /api/rpc`   | Body `{ "cmd": "<command>", "args": { ... } }`. Answers `{ "ok": <result> }` or `{ "error": "<message>" }`. |
| `GET /api/events` | WebSocket of `{ "event": "<name>", "payload": ... }`.                                                       |
| `GET /pi/<path>`  | Files from the plugin webserver; property inspector pages are adapted to run remotely.                      |
| `GET /pi-socket`  | WebSocket bridged to the plugin WebSocket server, used by property inspectors.                              |

Commands are the desktop frontend's Tauri commands with the same argument names (`get_devices`, `get_categories`, `get_profiles`, `get_selected_profile`, `set_selected_profile`, `rename_profile`, `delete_profile`, `set_profile_order`, `create_instance`, `move_instance`, `remove_instance`, `set_state`, `set_child_delay`, `trigger_virtual_press`, `get_replicated_slots`, `set_slot_replicated`, `get_device_layout`, `set_device_layout`, `make_info`, `switch_property_inspector`, `list_plugins`, `install_plugin` (URL only), `remove_plugin`, `reload_plugin`, `get_settings`, `set_settings`, `get_localisations`, `get_fonts`, `get_port_base`, `get_build_info`), plus these remote-only ones:

- `get_rendered_images { device }`: the latest image shown on each key, as `"Keypad.<n>"` → data URL
- `get_plugin_global_settings { plugin }` and `set_plugin_global_settings { plugin, settings }`: read and write a plugin's global settings; the setter notifies the plugin with `didReceiveGlobalSettings`
- `get_remote_info`: API version, OpenDeck version and plugin port

Commands that only make sense on the machine running OpenDeck (opening folders, file dialogs, installing from a local file) are not available. Selecting a profile that does not exist creates it, as in the desktop app. Changes made remotely are pushed to the desktop frontend so it re-renders the deck.

Events are the frontend's events (`devices`, `switch_profile`, `rerender_images`, `update_state`, `key_moved`, `show_alert`, `show_ok`, `plugin_reloaded`, `device_brightness`, `replicated_slots`) plus `key_image`, `profile_changed`, `profiles_changed`, and `resync` (sent when a client falls behind and should reload).

## Development

```sh
npm run mock:remote          # mock deck (a Streonor S6 with the Home Assistant plugin) on :57200, token "mocktoken0123456789"
npm run dev:remote           # Vite dev server proxying to :57200 (set OPENDECK_REMOTE to point elsewhere)
npx vite build remote-ui     # build into remote-ui/dist, which is embedded into OpenDeck at compile time
```

`npm run build` (run by `tauri build`) builds both the desktop frontend and this app. The Rust side lives in `src-tauri/src/remote/`.
