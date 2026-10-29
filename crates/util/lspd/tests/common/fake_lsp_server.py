import argparse
import json
import os
import re
import sys
from urllib.parse import quote, unquote, urlparse

parser = argparse.ArgumentParser()
for flag in ("--wedge-on", "--crash-on", "--fail-on", "--garble-before", "--define"):
    parser.add_argument(flag, action="append", default=[])
parser.add_argument("--pull-diagnostics", action="store_true")
cli = parser.parse_args()
defined = {}
for definition in cli.define:
    name, place = definition.split("=", 1)
    path, line = place.rsplit(":", 1)
    defined.setdefault(name, []).append((path, int(line) - 1))

texts, opens, save_counts, held = {}, {}, {}, {}
cancelled_since_open, wedged = set(), set()
refresh_supported = status_supported = loading = released = False
root_path = None
next_request_id = watcher_generation = 0


def write_frame(body):
    sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body)
    sys.stdout.buffer.flush()


def write(message):
    write_frame(json.dumps({"jsonrpc": "2.0", **message}, separators=(",", ":")).encode("utf-8"))


def respond(message, result):
    write({"id": message["id"], "result": result})


def error(message, code, text, data=None):
    write({"id": message.get("id"), "error": {"code": code, "message": text, **({"data": data} if data else {})}})


def notify(method, params):
    write({"method": method, "params": params})


def request(method, params):
    global next_request_id
    next_request_id += 1
    write({"id": f"fake-{next_request_id}", "method": method, "params": params})


def watcher_registration():
    return {"id": f"fake-watcher-{watcher_generation}", "method": "workspace/didChangeWatchedFiles"}


def register_watchers():
    global watcher_generation
    watcher_generation += 1
    watchers = [{"globPattern": f"{root_path}/**/*"}]
    registration = {**watcher_registration(), "registerOptions": {"watchers": watchers}}
    request("client/registerCapability", {"registrations": [registration]})


def read_message():
    length = None
    while line := sys.stdin.buffer.readline().decode("utf-8").strip():
        if line.startswith("Content-Length: "):
            length = int(line[len("Content-Length: "):])
    body = sys.stdin.buffer.read(length) if length is not None else b""
    return json.loads(body) if body else None


def uri_to_path(uri):
    return unquote(urlparse(uri).path)


def read_disk(path):
    try:
        with open(path, encoding="utf-8") as file:
            return file.read()
    except OSError:
        return ""


def span(line, start, end):
    return {"start": {"line": line, "character": start}, "end": {"line": line, "character": end}}


def location(uri, line, start, end):
    return {"uri": uri, "range": span(line, start, end)}


def diagnostic(message):
    return {"range": span(0, 0, 5), "severity": 1, "message": message}


def diagnostics_for(uri, text):
    found = [diagnostic("error token")] if "error" in text.lower() else []
    directory = uri_to_path(uri).rsplit("/", 1)[0]
    for line in text.splitlines():
        if line.startswith("// depends: ") and "error" in read_disk(f"{directory}/{line[12:].strip()}").lower():
            found.append(diagnostic("dependency error"))
    return found


def publish(uri, text):
    if cli.pull_diagnostics:
        return
    if "// hold-diagnostics" in text and not released:
        held[uri] = text
        return
    notify("textDocument/publishDiagnostics", {"uri": uri, "diagnostics": diagnostics_for(uri, text)})


def send_status():
    if status_supported:
        notify("experimental/serverStatus", {"health": "ok", "quiescent": not loading})


def word_at(text, line, character):
    rows = text.splitlines()
    if line >= len(rows):
        return ""
    row, start, end = rows[line], character, character
    while start > 0 and (row[start - 1].isalnum() or row[start - 1] == "_"):
        start -= 1
    while end < len(row) and (row[end].isalnum() or row[end] == "_"):
        end += 1
    return row[start:end]


def occurrences(uri, position):
    text = texts.get(uri, "")
    word = word_at(text, position["line"], position["character"])
    if not word:
        return []
    pattern = re.compile(rf"\b{re.escape(word)}\b")
    return [
        location(uri, line, match.start(), match.end())
        for line, row in enumerate(text.splitlines())
        for match in pattern.finditer(row)
    ]


while (message := read_message()) is not None:
    method = message.get("method")
    params = message.get("params", {})
    if method is None:
        continue
    if method in cli.crash_on:
        sys.exit(1)
    if method in cli.fail_on:
        error(message, -32603, "fake server rejected request")
        continue
    if method in cli.wedge_on:
        wedged.add(message.get("id"))
        notify("lspd/testing/requestReceived", {"method": method})
        continue
    if method in cli.garble_before:
        write_frame(b"{not json")

    uri = params.get("textDocument", {}).get("uri") if isinstance(params, dict) else None
    if method == "initialize":
        root_path = uri_to_path(params["rootUri"])
        loading = os.path.exists(os.path.join(root_path, ".fake-loading"))
        capabilities = params.get("capabilities", {})
        status_supported = (capabilities.get("experimental") or {}).get("serverStatusNotification", False)
        result = {"hoverProvider": True}
        if cli.pull_diagnostics:
            result["diagnosticProvider"] = {"interFileDependencies": True, "workspaceDiagnostics": False}
            refresh_supported = capabilities.get("workspace", {}).get("diagnostics", {}).get("refreshSupport", False)
        respond(message, {"capabilities": result})
    elif method == "initialized":
        send_status()
        if cli.pull_diagnostics:
            register_watchers()
    elif method == "workspace/didChangeWatchedFiles":
        if cli.pull_diagnostics and refresh_supported:
            request("workspace/diagnostic/refresh", None)
    elif method == "textDocument/diagnostic":
        if any(key in params and params[key] is None for key in ("identifier", "previousResultId")):
            error(message, -32602, "null value is not allowed")
            continue
        text = texts[uri] if uri in texts else read_disk(uri_to_path(uri))
        if "// cancel-first-pull" in text and uri not in cancelled_since_open:
            cancelled_since_open.add(uri)
            error(message, -32802, "server cancelled", {"retriggerRequest": True})
            continue
        respond(message, {"kind": "full", "items": diagnostics_for(uri, text)})
    elif method == "textDocument/didOpen":
        cancelled_since_open.discard(uri)
        opens[uri] = opens.get(uri, 0) + 1
        texts[uri] = params["textDocument"]["text"]
        publish(uri, texts[uri])
    elif method == "textDocument/didChange":
        texts[uri] = params["contentChanges"][-1]["text"]
        publish(uri, texts[uri])
    elif method == "textDocument/didSave":
        save_counts[uri] = save_counts.get(uri, 0) + 1
    elif method == "textDocument/didClose":
        opens[uri] = max(opens.get(uri, 0) - 1, 0)
        if opens[uri] == 0:
            texts.pop(uri, None)
    elif method == "$/cancelRequest":
        if params["id"] in wedged:
            wedged.discard(params["id"])
            write({"id": params["id"], "error": {"code": -32800, "message": "request cancelled"}})
            notify("lspd/testing/requestReceived", {"method": method})
    elif method == "fake/reregisterWatchers":
        previous = watcher_registration()
        register_watchers()
        request("client/unregisterCapability", {"unregisterations": [previous]})
        respond(message, None)
    elif method == "fake/finishLoading":
        loading = False
        send_status()
        respond(message, None)
    elif method == "fake/releaseDiagnostics":
        released = True
        for held_uri, text in held.items():
            publish(held_uri, text)
        held.clear()
        respond(message, None)
    elif method == "textDocument/hover":
        position = params["position"]
        value = (
            f"open_count={opens.get(uri, 0)}; line={position['line']}; character={position['character']}; "
            f"save_count={save_counts.get(uri, 0)}; text={texts.get(uri, '')}"
        )
        respond(message, None if loading else {"contents": {"kind": "plaintext", "value": value}})
    elif method == "textDocument/definition":
        word = word_at(texts.get(uri, ""), params["position"]["line"], params["position"]["character"])
        if word in defined:
            respond(message, [location("file://" + quote(path), line, 0, 0) for path, line in defined[word]])
        else:
            respond(message, occurrences(uri, params["position"])[:1])
    elif method == "textDocument/references":
        respond(message, occurrences(uri, params["position"]))
    elif method == "textDocument/documentSymbol":
        symbols = [("ExampleStruct", 23), ("example_fn", 12)]
        respond(message, [{"name": name, "kind": kind, "location": location(uri, 0, 0, len(name))} for name, kind in symbols])
    elif method == "textDocument/rename":
        edits = [{"range": found["range"], "newText": params["newName"]} for found in occurrences(uri, params["position"])]
        respond(message, {"changes": {uri: edits}})
    elif "id" in message:
        respond(message, None)
