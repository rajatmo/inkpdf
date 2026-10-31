"""Runs the real-input checks against the built app on one platform and says what passed. The same checks run everywhere (scenarios.py); only the
way input is sent differs (win.py, linux.py, android.py). They drive the REAL app with the real devices the platform has, so they take over
the mouse and keyboard on Windows (an unlocked desktop; do not touch the machine) and a virtual display on Linux (see run.ps1).

    python tools/e2e/run.py windows              all the scenarios
    python tools/e2e/run.py windows --only pages one of them (devices, pages, reader, forms)
    python tools/e2e/run.py linux                (inside xvfb-run, in WSL or on Linux: run.ps1 does that)
    python tools/e2e/run.py android              (an emulator or device on adb, the APK installed by run.ps1)

INKPDF_E2E_SHOTS=folder keeps a picture of every look taken. The exit status is 0 only if nothing failed.
"""

import argparse
import os
import sys
import time
import traceback

sys.dont_write_bytecode = True  # (nothing is written inside the repository)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from core import Report, Session  # noqa: E402
from scenarios import ALL  # noqa: E402


def driver_for(platform):
    if platform == "windows":
        from win import Windows

        return Windows()
    if platform == "linux":
        from linux import Linux

        return Linux()
    from android import Android

    return Android()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("platform", choices=["windows", "linux", "android"])
    ap.add_argument("--only", nargs="*", choices=sorted(ALL))
    args = ap.parse_args()
    driver = driver_for(args.platform)
    clock = time.time()
    total_ok = total_failed = total_skipped = 0
    for name in args.only or list(ALL):
        report = Report()
        session = Session(driver, report)
        crashed = None
        try:
            ALL[name](session)
        except Exception:  # a scenario that could not run is a failure, with the reason
            crashed = traceback.format_exc()
        finally:
            try:
                driver.stop()
            except Exception:
                pass
        failed = report.failed + (1 if crashed else 0)
        total_ok, total_failed, total_skipped = total_ok + report.ok, total_failed + failed, total_skipped + report.skipped
        print(f"== {args.platform} {name:<8} {report.ok:2} ok, {report.failed} failed, {report.skipped} skipped" + (", THE SCENARIO ITSELF FAILED" if crashed else ""))
        for line in report.lines:
            if line.startswith("FAIL:"):
                print("         " + line)
        if crashed:
            print("         " + crashed.strip().replace("\n", "\n         "))
    print(f"total ({args.platform}): {total_ok} ok, {total_failed} failed, {total_skipped} skipped, in {time.time() - clock:.0f} s")
    sys.exit(1 if total_failed else 0)


if __name__ == "__main__":
    main()
