# Start and Stage performance quick check

The Windows fixture prints Start layout and counts, Stage software projection draw time and optional WGC first-frame elapsed time. These observations are **not** a matched baseline comparison and do not measure desktop GPU or DWM. WGC and the CPU projection renderer are test-only; there is no periodic product capture loop.

The matched owner-machine comparison remains **NOT_TESTED**. Use the same geometry, DPI, focus and approved fixture windows in each candidate. Measure Deskland CPU, private bytes, working set, handles, Deskland and DWM GPU engine utilization, preview latency and animation frame timings. Reject mismatched wallpaper coverage or foreground and do not reuse a pre-redesign benchmark. No four-hour run is requested in this round.
