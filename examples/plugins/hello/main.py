import json
import sys


def query(params):
    q = str(params.get("query") or "").strip() or "world"
    greeting = f"Hello, {q}"
    start = greeting.find(q)
    return {
        "items": [
            {
                "title": greeting,
                "subtitle": "Enter copies the greeting",
                "highlight": list(range(start, start + len(q))),
                "action": {"type": "copy", "text": greeting},
            },
            {
                "title": f"Run echo {q}",
                "subtitle": "Runs it in command mode",
                "action": {"type": "run", "command": f"echo {q}"},
            },
            {
                "title": "Uppercase it",
                "subtitle": "Replaces the query",
                "action": {"type": "plugin", "data": {"text": q}},
            },
        ]
    }


def action(params):
    data = params.get("data") or {}
    text = str(data.get("text") or "")
    return {"hide": False, "query": "hi " + text.upper()}


METHODS = {"query": query, "action": action}


def main():
    sys.stdin.reconfigure(encoding="utf-8")
    sys.stdout.reconfigure(encoding="utf-8")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            print(f"bad request: {e}", file=sys.stderr, flush=True)
            continue
        req_id = req.get("id")
        method = req.get("method")
        handler = METHODS.get(method)
        if handler is None:
            reply = {"id": req_id, "error": {"message": f"unknown method {method}"}}
        else:
            reply = {"id": req_id, "result": handler(req.get("params") or {})}
        print(json.dumps(reply, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
