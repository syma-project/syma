#!/usr/bin/env python3
"""Syma Python bridge — reads a JSON request from stdin, calls the specified
function, and writes the JSON result (or error) to stdout."""
import sys
import json
import importlib
import traceback


def main():
    try:
        req = json.load(sys.stdin)
        module_name = req["module"]
        func_name = req["func"]
        args = req.get("args", [])

        mod = importlib.import_module(module_name)
        fn = getattr(mod, func_name)
        result = fn(*args)

        # Convert numpy/etc. types to plain Python where possible.
        try:
            import numbers
            if isinstance(result, numbers.Integral):
                result = int(result)
            elif isinstance(result, numbers.Real):
                result = float(result)
        except ImportError:
            pass

        json.dump({"ok": result}, sys.stdout)
        sys.stdout.flush()
    except Exception as exc:
        json.dump({"error": traceback.format_exc()}, sys.stdout)
        sys.stdout.flush()
        sys.exit(1)


if __name__ == "__main__":
    main()
