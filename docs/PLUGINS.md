# Plugins

A plugin is a program that nebucat starts as a child process. nebucat writes JSON requests to its stdin and reads JSON responses from its stdout. Any language works, as long as the program reads and writes UTF-8 lines.

The wire types live in `crates/nebucat-core/src/plugin_proto.rs`. The process handling lives in `crates/nebucat/src/plugins.rs`. This document describes both; change them together.

## Install

Put each plugin in its own folder under the plugins directory:

```
%APPDATA%\nebucat\plugins\<dir>\plugin.toml
```

nebucat scans this folder at startup and again whenever anything under `plugins\` changes. No restart is needed. Folders without a `plugin.toml` are skipped silently.

## Manifest: `plugin.toml`

| Field | Type | Meaning |
|---|---|---|
| `id` | string | Identifier used in log lines (`plugin <id>: ...`). |
| `name` | string | Display name. It is the row label and the prefix of every error message. |
| `keyword` | string | What the user types, followed by a space, to query this plugin. |
| `command` | array of strings | Program and arguments. `command[0]` is the program, the rest are its arguments. |

All four fields are required. Unknown keys are an error.

`command[0]` resolution:

- An absolute path is used as is.
- A relative path with a directory part, such as `bin\tool.exe`, is resolved against the plugin folder.
- A bare name, such as `python`, is looked up the way `std::process::Command` does on Windows (`PATH`).

### Keyword rules

- The keyword must not be empty, must not contain whitespace, and must not start with `>`, `/` or `!`. Those prefixes belong to command mode, file search and power options.
- Keywords must be unique. Folders are processed in name order, and the first folder that claims a keyword keeps it.

A rejected manifest does not load. nebucat shows a banner labeled `Plugin` with one of these messages, where `<dir>` is the plugin folder name:

| Cause | Banner text |
|---|---|
| `plugin.toml` can't be read | `Plugin <dir>: <OS error>` |
| Invalid TOML, missing or unknown field | `Plugin <dir>: line <n>: <parser message>`, or `Plugin <dir>: <parser message>` when the parser reports no position |
| Keyword empty, has whitespace, or starts with `>`, `/` or `!` | `Plugin <dir>: keyword "<k>" not allowed` |
| Keyword already taken | `Plugin <dir>: keyword "<k>" already used by <other dir>` |

The banner stays until the next scan that no longer finds the problem.

## Invocation

The plugin answers when the input starts with its keyword followed by a space. With keyword `hi`:

| Input | Result |
|---|---|
| `hi` | App search for `hi`; the plugin is not queried. |
| `hi ` | Plugin query with `""`. |
| `hi cat` | Plugin query with `"cat"`. |
| `hi   cat ` | Plugin query with `"cat "` (leading spaces trimmed, trailing spaces kept). |

## Process lifetime

- **Lazy start.** The process starts on the first query or action for its keyword, not at nebucat startup.
- **One process per plugin.** It stays alive and serves every later request.
- **Working directory** is the plugin folder.
- **No console window.** The process starts with `CREATE_NO_WINDOW`. stdin, stdout and stderr are pipes.
- **Crash restart.** When the plugin's stdout closes, nebucat waits up to 500 ms for the process to exit, then kills it. Every pending request fails with the stop message (see [Errors](#errors-shown-to-the-user)). The next request starts a fresh process.
- **Write failure.** If writing a request to stdin fails, nebucat kills the process; the stop handling above follows.
- **Manifest change.** If the folder, keyword or any manifest field changes, the running process is killed. The next query starts the new version. A removed plugin is killed.
- **Quit.** nebucat kills all plugin processes when it exits. There is no shutdown message; plugins must not rely on one.

## Framing

Newline-delimited JSON over stdio, UTF-8.

- Each message is one JSON object on one line, ended by `\n`. nebucat never writes a newline inside a request; strings carry `\n` escaped.
- A trailing `\r` before `\n` is accepted.
- Flush stdout after every response. Buffered output looks like a hung plugin and hits the timeout.
- stdout lines that are not valid JSON, or not a valid response object, are ignored. nebucat logs them at debug level.
- Invalid UTF-8 on stdout is replaced with U+FFFD before parsing.
- stderr is free-form text; see [stderr](#stderr).

Every request carries an integer `id`. The response must echo the same `id`. IDs increase by one per request within a process, starting at 1, and are shared by `query` and `action`. A restarted process starts again at 1. Treat `id` as opaque: echo it, nothing more. Responses may arrive in any order.

A response has `result` on success or `error` on failure:

```json
{"id":1,"result":{...}}
{"id":1,"error":{"message":"what went wrong"}}
```

If both are present, `error` wins. A plugin must answer every request it receives, including methods it does not know (reply with `error`).

## `query`

Sent 80 ms after the last keystroke while the input is in this plugin's mode. `params.query` is the text after the keyword and its space.

Request:

```json
{"id":1,"method":"query","params":{"query":"cat"}}
```

Response:

```json
{"id":1,"result":{"items":[{"title":"Hello, cat","highlight":[7,8,9],"action":{"type":"copy","text":"Hello, cat"}}]}}
```

`result.items` is required; it may be empty. An empty list shows the notice `<name>: no results.` nebucat shows at most the first 9 items.

### Item

| Field | Type | Required | Meaning |
|---|---|---|---|
| `title` | string | yes | Row title. |
| `subtitle` | string | no | Smaller muted line under the title. |
| `icon` | string | no | Path to a PNG, absolute or relative to the plugin folder, shown at 24 by 24 px. |
| `highlight` | array of integers | no | Character indices into `title` (Unicode scalar values, not bytes), ascending. Those characters render SemiBold in the plugin accent. |
| `action` | object | no | What `Enter` does; see [Actions](#actions). Without it the row can be selected but `Enter` does nothing. |

Each row shows the plugin `name` as its source label.

## Actions

`action` is an object with a `type` field. There are five types:

| `type` | Fields | `Enter` | `Ctrl+Enter` |
|---|---|---|---|
| `open_url` | `url` | Opens the URL with the default handler, then hides. | Same as `Enter`. |
| `open_path` | `path` | `ShellExecuteW` `open` on the path, then hides. | Opens Explorer with the file selected, then hides. |
| `copy` | `text` | Copies `text` to the clipboard, then hides. | Same as `Enter`. |
| `run` | `command` | Runs `command` in command mode and shows its output panel, like `><command>`. | Opens it in Windows Terminal (`cmd /k`), then hides. |
| `plugin` | `data` (any JSON) | Sends an `action` request to the plugin. | Same as `Enter`. |

```json
{"type":"open_url","url":"https://example.com"}
{"type":"open_path","path":"C:\\Users\\me\\notes.txt"}
{"type":"copy","text":"Hello, cat"}
{"type":"run","command":"echo cat"}
{"type":"plugin","data":{"text":"cat"}}
```

`open_url`, `open_path`, `copy` and `run` run inside nebucat; the plugin is not contacted. If `open_path` fails, the notice reads `Couldn't open <path> (error <code>).` and the window stays open.

## `action`

Sent when the user activates a row whose action type is `plugin`. `params.data` is the item's `data`, unchanged.

Request:

```json
{"id":2,"method":"action","params":{"data":{"text":"cat"}}}
```

Response (`ActionResult`):

```json
{"id":2,"result":{"hide":false,"query":"hi CAT"}}
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `hide` | bool | `false` | Hide the launcher. |
| `query` | string | absent | Replace the input with this text and re-route it. |

- If `query` is set, nebucat replaces the input and handles it like typed text. It can point back to this plugin (`"hi CAT"`), to another plugin, or to another mode such as `">dir"`. `hide` is ignored in this case.
- Otherwise, if `hide` is `true`, the launcher hides.
- Otherwise nothing changes.

`{}` is a valid result and does nothing.

## Timing

- **Debounce.** A `query` is sent 80 ms after the last change to the input. Faster typing sends fewer queries.
- **Timeout.** Every request, `query` or `action`, fails 2000 ms after it was sent if no response arrived. The process is not killed. A late response for that `id` is ignored.
- **Stale responses.** When a new `query` is sent, the previous unanswered `query` is dropped. Its response, if it arrives, is ignored. The UI also ignores any reply that arrives after the input has changed. `action` requests are not dropped by later queries.

A plugin that handles requests one at a time should answer quickly, because each keystroke pause sends a new `query`.

## Errors shown to the user

These appear as a notice row in place of the results. `<name>` is the manifest `name`.

| Situation | Text |
|---|---|
| Response has `error` | `<name>: <message>` |
| `result` has no `items` | `<name>: result has no "items" field` |
| `items` or `ActionResult` has the wrong shape | `<name>: <serde_json error>` |
| `items` is empty | `<name>: no results.` |
| No response within 2 s | `<name> didn't answer in 2 s.` |
| Process stopped with stderr output | `<name> stopped: <last non-empty stderr line>` |
| Process stopped without stderr output | `<name> stopped: exit <code>` |
| Process stopped, exit code unavailable | `<name> stopped: exit unknown` |
| Spawn failed | `Couldn't start <name>: <OS error>` |
| `command` is empty | `Couldn't start <name>: command is empty` |

## stderr

nebucat reads stderr continuously, so a plugin can write to it freely without blocking. It keeps the last 20 lines in a ring buffer. When the process stops, the last non-empty line becomes the stop message. Each stderr line is also logged at debug level. The log file at `%LOCALAPPDATA%\nebucat\logs\nebucat.log` records at `Info`, so it contains plugin start (`plugin <id>: started pid <n>`), stop (`plugin <id>: stopped: <detail>`) and spawn or write failures, but not stderr lines.

Write diagnostics to stderr, never to stdout.

## Worked example: `examples/plugins/hello`

Install it by copying `examples/plugins/hello` to `%APPDATA%\nebucat\plugins\hello`. It needs `python` on `PATH` and uses only the standard library.

`plugin.toml`:

```toml
id = "hello"
name = "Hello"
keyword = "hi"
command = ["python", "main.py"]
```

`main.py` reads stdin line by line, dispatches on `method`, and prints one JSON reply per request with `flush=True`. It reconfigures stdin and stdout to UTF-8, because Python on Windows otherwise uses the console code page. Unknown methods get an `error` reply. Bad JSON input is reported on stderr and skipped.

### Sample exchange

The user types `hi cat`. 80 ms later nebucat starts `python main.py` in the plugin folder and sends:

```json
{"id":1,"method":"query","params":{"query":"cat"}}
```

The plugin replies on one line (shown wrapped here):

```json
{"id": 1, "result": {"items": [
  {"title": "Hello, cat", "subtitle": "Enter copies the greeting", "highlight": [7, 8, 9], "action": {"type": "copy", "text": "Hello, cat"}},
  {"title": "Run echo cat", "subtitle": "Runs it in command mode", "action": {"type": "run", "command": "echo cat"}},
  {"title": "Uppercase it", "subtitle": "Replaces the query", "action": {"type": "plugin", "data": {"text": "cat"}}}
]}}
```

nebucat shows three rows labeled `Hello`. In the first row, `cat` is highlighted.

- `Enter` on `Hello, cat` copies `Hello, cat` and hides the launcher. No request is sent.
- `Enter` on `Run echo cat` runs `echo cat` in command mode. The output panel shows `cat`. No request is sent.
- `Enter` on `Uppercase it` sends:

  ```json
  {"id":2,"method":"action","params":{"data":{"text":"cat"}}}
  ```

  The plugin replies:

  ```json
  {"id": 2, "result": {"hide": false, "query": "hi CAT"}}
  ```

  nebucat sets the input to `hi CAT`, and 80 ms later sends `{"id":3,"method":"query","params":{"query":"CAT"}}`.

If you kill `python.exe` in Task Manager, a request still pending at that moment fails with `Hello stopped: exit <code>` (or the last stderr line). The next query starts a new process.
