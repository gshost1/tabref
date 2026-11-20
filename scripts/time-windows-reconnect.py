import os
from pathlib import Path
import statistics
import subprocess
import time


def command(binary, *args):
    output = Path(os.environ["RUNNER_TEMP"]) / f"{binary.name}-{time.time_ns()}.log"
    with output.open("wb") as log:
        result = subprocess.run(
            [binary, *args], stdin=subprocess.DEVNULL, stdout=log,
            stderr=subprocess.STDOUT, timeout=40, check=False,
        )
    response = output.read_text(encoding="utf-8", errors="replace")
    if result.returncode:
        raise RuntimeError(f"{binary.name} {args}: {response}")
    return response


def main():
    temp = Path(os.environ["RUNNER_TEMP"])
    before = temp / "before.exe"
    after = Path("target/debug/tabref.exe").resolve()
    os.environ["TABREF_HOME"] = str(temp / "tabref-reconnect-timing")
    os.environ["TABREF_NO_DAEMON"] = "1"
    os.environ["TABREF_CHROME"] = str(
        Path(os.environ["ProgramFiles"]) / "Google/Chrome/Application/chrome.exe"
    )
    os.environ.pop("TABREF_CDP", None)

    print("Launching Chrome", flush=True)
    command(before, "--timeout", "30000", "open", "about:blank", "--launch")
    print("Chrome launched, measuring tabs", flush=True)
    try:
        samples = {before: [], after: []}
        for trial in range(13):
            order = (before, after) if trial % 2 == 0 else (after, before)
            for binary in order:
                start = time.perf_counter()
                output = command(binary, "--timeout", "5000", "tabs")
                elapsed = (time.perf_counter() - start) * 1000
                if "about:blank" not in output:
                    raise RuntimeError(f"{binary.name} did not list the expected tab: {output}")
                if trial >= 2:
                    samples[binary].append(elapsed)
            print(f"Completed paired trial {trial + 1}", flush=True)
        for binary, times in samples.items():
            print(
                f"{binary.name}: n={len(times)} min={min(times):.2f}ms "
                f"median={statistics.median(times):.2f}ms max={max(times):.2f}ms",
                flush=True,
            )
    finally:
        command(after, "--timeout", "5000", "close", "--browser")


if __name__ == "__main__":
    main()
