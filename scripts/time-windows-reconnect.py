import os
from pathlib import Path
import statistics
import subprocess
import time


def command(binary, *args):
    result = subprocess.run(
        [binary, *args], stdin=subprocess.DEVNULL, capture_output=True, text=True,
        timeout=40, check=False,
    )
    if result.returncode:
        raise RuntimeError(f"{binary.name} {args}: {result.stdout} {result.stderr}")
    return result.stdout


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
